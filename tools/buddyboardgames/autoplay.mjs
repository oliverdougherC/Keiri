#!/usr/bin/env node
import { execFileSync } from "node:child_process";
import { appendFileSync, mkdirSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = dirname(fileURLToPath(import.meta.url));
const repoRoot = resolve(__dirname, "../..");

const args = parseArgs(process.argv.slice(2));
const dryRun = !args.execute;
const url = args.url || "https://www.buddyboardgames.com/yahtzee";
const oracleEndgame = args.oracleEndgame || "2";
const agent = args.agent || "auto";
const table = args.table || "target/keiri_tables/bbg-anchor-v1.bin";
const traceDir = args.traceDir || "target/bbg-traces";
const joinOnly = Boolean(args.joinOnly);
const loop = Boolean(args.loop);

let chromium;
try {
  ({ chromium } = await import("playwright"));
} catch (error) {
  console.error("Playwright is required for BuddyBoardGames autoplay.");
  console.error("Run with: npx --yes --package playwright node tools/buddyboardgames/autoplay.mjs --dry-run");
  console.error(error.message);
  process.exit(2);
}

const browser = await chromium.launch({ headless: args.headed ? false : true });
const page = await browser.newPage();

try {
  await page.goto(url, { waitUntil: "domcontentloaded" });

  if (args.player || args.room) {
    await joinRoom(page, args.player, args.room, {
      startGame: args.startGame,
      autoStartSolo: args.autoStartSolo,
    });
  }

  if (loop) {
    console.log(`joined: ${args.room || "(page default room)"}`);
    console.log(`player: ${args.player || "(page default player)"}`);
    console.log("status: autoplay-loop");
    await autoplayLoop(page, {
      oracleEndgame,
      agent,
      table,
      traceDir,
      pollMs: Number(args.pollMs || 1000),
      maxActions: args.maxActions ? Number(args.maxActions) : Infinity,
      dryRun,
    });
  } else if (joinOnly) {
    console.log(`joined: ${args.room || "(page default room)"}`);
    console.log(`player: ${args.player || "(page default player)"}`);
    console.log("status: connected");
    if (args.keepOpen) {
      console.log("keeping browser open; press Ctrl-C to stop");
      await waitUntilStopped(page);
    }
  } else {
    await page.waitForTimeout(Number(args.waitMs || 500));
    const snapshot = await readSnapshot(page);
    if (args.probe) {
      console.log(JSON.stringify(snapshot.probe, null, 2));
    }
    const advice = runKeiriAdvice(snapshot.tokens, { oracleEndgame, agent, table });

    console.log(advice.raw.trim());

    if (dryRun) {
      console.log("dry_run: true");
    } else {
      const before = snapshot.tokens;
      await applyAdvice(page, advice);
      const after = await readSnapshot(page).catch((error) => ({ error: error.message }));
      writeTrace(traceDir, { before, advice: advice.raw, parsed: advice, after, dryRun: false });
      console.log("executed: true");
    }
  }
} finally {
  await browser.close();
}

function parseArgs(argv) {
  const parsed = {};
  for (const arg of argv) {
    if (arg === "--execute") parsed.execute = true;
    else if (arg === "--dry-run") parsed.execute = false;
    else if (arg === "--headed") parsed.headed = true;
    else if (arg === "--join-only") parsed.joinOnly = true;
    else if (arg === "--loop") parsed.loop = true;
    else if (arg === "--keep-open") parsed.keepOpen = true;
    else if (arg === "--start-game") parsed.startGame = true;
    else if (arg === "--auto-start-solo") parsed.autoStartSolo = true;
    else if (arg === "--probe") parsed.probe = true;
    else if (arg.startsWith("--url=")) parsed.url = arg.slice("--url=".length);
    else if (arg.startsWith("--player=")) parsed.player = arg.slice("--player=".length);
    else if (arg.startsWith("--room=")) parsed.room = arg.slice("--room=".length);
    else if (arg.startsWith("--wait-ms=")) parsed.waitMs = arg.slice("--wait-ms=".length);
    else if (arg.startsWith("--poll-ms=")) parsed.pollMs = arg.slice("--poll-ms=".length);
    else if (arg.startsWith("--max-actions=")) parsed.maxActions = arg.slice("--max-actions=".length);
    else if (arg.startsWith("--oracle-endgame=")) parsed.oracleEndgame = arg.slice("--oracle-endgame=".length);
    else if (arg.startsWith("--agent=")) parsed.agent = arg.slice("--agent=".length);
    else if (arg.startsWith("--table=")) parsed.table = arg.slice("--table=".length);
    else if (arg.startsWith("--trace-dir=")) parsed.traceDir = arg.slice("--trace-dir=".length);
    else throw new Error(`Unknown argument: ${arg}`);
  }
  return parsed;
}

async function autoplayLoop(page, options) {
  let actions = 0;
  let lastSkip = "";

  while (actions < options.maxActions) {
    const status = await readLoopStatus(page);
    if (status.ended) {
      console.log("status: game-ended");
      return;
    }
    if (!status.ready) {
      const skip = `waiting: ${status.reason}`;
      if (skip !== lastSkip) {
        console.log(skip);
        lastSkip = skip;
      }
      await page.waitForTimeout(options.pollMs);
      continue;
    }

    try {
      const snapshot = await readSnapshot(page);
      const advice = runKeiriAdvice(snapshot.tokens, options);
      console.log(advice.raw.trim());
      if (options.dryRun) {
        console.log("dry_run: true");
      } else {
        const before = snapshot.tokens;
        await applyAdvice(page, advice);
        const after = await readSnapshot(page).catch((error) => ({ error: error.message }));
        writeTrace(options.traceDir, { before, advice: advice.raw, parsed: advice, after, dryRun: false });
        actions += 1;
        console.log(`executed: true actions=${actions}`);
      }
    } catch (error) {
      console.log(`waiting: ${error.message}`);
    }

    await page.waitForTimeout(options.pollMs);
  }

  console.log(`status: max-actions reached (${options.maxActions})`);
}

async function readLoopStatus(page) {
  return page.evaluate(() => {
    if (typeof thisGame === "undefined" || !thisGame) {
      return { ready: false, ended: false, reason: "thisGame unavailable" };
    }
    const game = thisGame;
    if (game.gameState === "ENDED" || game.turnsLeft === 0) {
      return { ready: false, ended: true, reason: "ended" };
    }
    if (game.gameState !== "STARTED") {
      return { ready: false, ended: false, reason: `gameState=${game.gameState}` };
    }
    if (game.isSpectator) {
      return { ready: false, ended: false, reason: "spectator" };
    }
    if (game.meIdx !== game.turnIdx) {
      return { ready: false, ended: false, reason: `not my turn me=${game.meIdx} turn=${game.turnIdx}` };
    }
    if (game.rollResultPending || window.yahtzeeRollDiceTimeoutId) {
      return { ready: false, ended: false, reason: "roll/update pending" };
    }
    return { ready: true, ended: false, reason: "ready" };
  });
}

function waitUntilStopped(page) {
  return new Promise((resolve) => {
    let done = false;
    const finish = () => {
      if (!done) {
        done = true;
        resolve();
      }
    };
    process.once("SIGINT", finish);
    process.once("SIGTERM", finish);
    page.once("close", finish);
  });
}

async function joinRoom(page, player, room, options = {}) {
  if (player) await page.locator("#player").fill(player);
  if (room) await page.locator("#room").fill(room);
  await page.locator("#start-game").click();
  await page.waitForTimeout(800);

  const shouldStart =
    options.startGame || (options.autoStartSolo && await isSoloLobby(page));
  if (shouldStart) {
    await startLobbyGame(page);
  }
}

async function isSoloLobby(page) {
  return page.evaluate(() => {
    const game = globalThis.thisGame;
    return Boolean(
      game &&
      game.gameState === "LOBBY" &&
      Array.isArray(game.players) &&
      game.players.length === 1 &&
      game.meIdx === 0
    );
  }).catch(() => false);
}

async function startLobbyGame(page) {
  const start = page.locator("#start-game-lobby");
  if (await start.isVisible().catch(() => false)) {
    await start.click();
    await page.waitForTimeout(800);
  }
}

async function readSnapshot(page) {
  const data = await page.evaluate(() => {
    if (typeof thisGame === "undefined" || !thisGame) {
      throw new Error("BuddyBoardGames thisGame is not available");
    }
    const game = thisGame;
    if (!game.dice || !Array.isArray(game.dice.dice)) {
      throw new Error("BuddyBoardGames dice are not readable");
    }
    if (game.meIdx < 0 || !game.players || !game.players[game.meIdx]) {
      throw new Error("BuddyBoardGames current player is not readable");
    }

    const player = game.players[game.meIdx];
    const rows = [];
    for (let clientRow = 0; clientRow <= 13; clientRow += 1) {
      if (clientRow === 6) continue;
      const serverRow =
        typeof game.clientToServerRowIdx === "function"
          ? game.clientToServerRowIdx(clientRow)
          : clientRow + 1;
      const row = player.score.rows[serverRow];
      if (!row) throw new Error(`Missing score row ${clientRow}`);
      rows.push(`${clientRow}:${Number(row.value || 0)}:${row.selected ? 1 : 0}`);
    }

    return {
      gameState: String(game.gameState || ""),
      meIdx: Number(game.meIdx),
      turnIdx: Number(game.turnIdx),
      isSpectator: Boolean(game.isSpectator),
      rollPending: Boolean(game.rollResultPending || window.yahtzeeRollDiceTimeoutId),
      rollsUsed: Number(game.playerDiceRollCount || 0),
      dice: game.dice.dice.map((die) => Number(die.value || 1)).join(","),
      selected: game.dice.dice.map((die) => (die.selected ? 1 : 0)).join(","),
      rows: rows.join(","),
    };
  });

  return {
    tokens: [
      `state=${data.gameState}`,
      `me=${data.meIdx}`,
      `turn=${data.turnIdx}`,
      `spectator=${data.isSpectator}`,
      `pending=${data.rollPending}`,
      `dice=${data.dice}`,
      `selected=${data.selected}`,
      `rolls=${data.rollsUsed}`,
      `rows=${data.rows}`,
    ],
    probe: data,
  };
}

function runKeiriAdvice(tokens, options) {
  const adviceArgs = [
    "run",
    "--quiet",
    "--",
    "bbg-advise",
    `oracle_endgame=${options.oracleEndgame}`,
    `agent=${options.agent}`,
    ...tokens,
  ];
  if (
    options.agent === "exact-table" ||
    options.agent === "exact" ||
    options.agent === "table" ||
    options.agent === "auto" ||
    options.agent === "best" ||
    options.agent === "smartest"
  ) {
    adviceArgs.splice(6, 0, `table=${options.table}`);
  }
  const output = execFileSync(
    "cargo",
    adviceArgs,
    { cwd: repoRoot, encoding: "utf8" },
  );
  return parseAdvice(output);
}

function parseAdvice(raw) {
  const fields = new Map();
  for (const line of raw.split(/\r?\n/)) {
    const index = line.indexOf(":");
    if (index > 0) fields.set(line.slice(0, index).trim(), line.slice(index + 1).trim());
  }
  return {
    raw,
    action: fields.get("action") || "",
    selector: fields.get("selector") || "",
    expectedValue: fields.get("expected_value") || "",
    source: fields.get("source") || "",
    state: fields.get("state") || "",
    alternatives: fields.get("alternatives") || "",
    toggleDice: (fields.get("toggle_dice") || "")
      .split(",")
      .filter(Boolean)
      .map((value) => Number(value)),
  };
}

function writeTrace(traceDir, event) {
  const dir = resolve(repoRoot, traceDir);
  mkdirSync(dir, { recursive: true });
  const file = join(dir, `${new Date().toISOString().slice(0, 10)}.jsonl`);
  appendFileSync(file, `${JSON.stringify({ timestamp: new Date().toISOString(), ...event })}\n`);
}

async function applyAdvice(page, advice) {
  if (advice.action.startsWith("roll")) {
    for (const die of advice.toggleDice) {
      await domClick(page, `#die-${die}`);
      await page.waitForTimeout(100);
    }
    await domClick(page, "#roll-dice");
    return;
  }

  if (advice.action.startsWith("score")) {
    if (!advice.selector) throw new Error("Score advice did not include a selector");
    await domClick(page, advice.selector);
    await page.waitForTimeout(250);
    const confirm = page.locator("#confirm-score-modal-confirmed");
    if (await confirm.isVisible().catch(() => false)) {
      await domClick(page, "#confirm-score-modal-confirmed");
    }
    return;
  }

  throw new Error(`Unsupported advice action: ${advice.action}`);
}

async function domClick(page, selector) {
  await page.evaluate((selector) => {
    const element = document.querySelector(selector);
    if (!element) throw new Error(`Missing selector ${selector}`);
    element.dispatchEvent(new MouseEvent("click", { bubbles: true, cancelable: true, view: window }));
  }, selector);
}
