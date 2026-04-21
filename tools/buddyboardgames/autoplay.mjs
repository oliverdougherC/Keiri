#!/usr/bin/env node
import { execFileSync } from "node:child_process";
import { appendFileSync, existsSync, mkdirSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const modulePath = fileURLToPath(import.meta.url);
const __dirname = dirname(modulePath);
const repoRoot = resolve(__dirname, "../..");

if (isMainModule()) {
  await main().catch((error) => {
    console.error(error.message);
    process.exit(1);
  });
}

export const DEFAULT_PLAYER_NAME = "Keiri";

export function parseArgs(argv) {
  const parsed = {};
  for (const arg of argv) {
    if (arg === "--execute") parsed.execute = true;
    else if (arg === "--dry-run") parsed.execute = false;
    else if (arg === "--headed") parsed.headed = true;
    else if (arg === "--join-only") parsed.joinOnly = true;
    else if (arg === "--loop") parsed.loop = true;
    else if (arg === "--restart-games") parsed.restartGames = true;
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

export function classifyProbeState(probe) {
  const gameState = String(probe.gameState || "");
  if (gameState === "ENDED" || probe.turnsLeft === 0) {
    return { ready: false, ended: true, reason: "ended" };
  }
  if (gameState !== "STARTED") {
    return {
      ready: false,
      ended: false,
      reason: gameState ? `gameState=${gameState}` : "gameState=unknown",
    };
  }
  if (probe.isSpectator) {
    return { ready: false, ended: false, reason: "spectator" };
  }
  if (Number(probe.meIdx) !== Number(probe.turnIdx)) {
    return {
      ready: false,
      ended: false,
      reason: `not my turn me=${probe.meIdx} turn=${probe.turnIdx}`,
    };
  }
  if (probe.rollPending) {
    return { ready: false, ended: false, reason: "roll/update pending" };
  }
  return { ready: true, ended: false, reason: "ready" };
}

export function formatGuardStatus(status) {
  if (status.ended) {
    return ["status: game-ended"];
  }
  if (status.ready) {
    return ["status: ready"];
  }
  return ["status: waiting", `reason: ${status.reason}`];
}

export function meanScore(scores) {
  if (!scores.length) return 0;
  return scores.reduce((sum, score) => sum + score, 0) / scores.length;
}

export function formatScoreSummary(scores, options = {}) {
  const reason = options.reason || "stopped";
  if (!scores.length) {
    const lines = [
      `session stopped: ${reason}`,
      "completed_games: 0",
      "highest_score: n/a",
      "mean_score: n/a",
    ];
    if (options.chartUnavailableReason) {
      lines.push(`score_graph_png: unavailable (${options.chartUnavailableReason})`);
    }
    return lines.join("\n");
  }

  const highest = Math.max(...scores);
  const mean = meanScore(scores);
  const lines = [
    `session stopped: ${reason}`,
    `completed_games: ${scores.length}`,
    `highest_score: ${highest}`,
    `mean_score: ${mean.toFixed(2)}`,
  ];
  if (options.chartPath) {
    lines.push(`score_graph_png: ${options.chartPath}`);
  } else if (options.chartUnavailableReason) {
    lines.push(`score_graph_png: unavailable (${options.chartUnavailableReason})`);
  }
  return lines.join("\n");
}

async function main() {
  const args = parseArgs(process.argv.slice(2));
  const dryRun = !args.execute;
  const url = args.url || "https://www.buddyboardgames.com/yahtzee";
  const oracleEndgame = args.oracleEndgame || "2";
  const agent = args.agent || "auto";
  const table = args.table || "target/keiri_tables/bbg-anchor-v1.bin";
  const traceDir = args.traceDir || "target/bbg-traces";
  const joinOnly = Boolean(args.joinOnly);
  const loop = Boolean(args.loop);
  const hasExplicitPlayer = Boolean(args.player);
  const player = args.player || DEFAULT_PLAYER_NAME;
  const room = args.room;
  const playerLabel = room || hasExplicitPlayer ? player : "(page default player)";
  const session = createLoopSession();
  const stopController = createStopController({
    onRequestStop: (reason) => {
      process.stdout.write(`\nstop requested: ${reason}; finishing current step...\n`);
    },
    onForceStop: (reason) => {
      process.stdout.write(`\nforcing shutdown after ${reason}\n`);
      process.stdout.write(`${session.emitSummary({ reason, forceChartAttempt: true })}\n`);
    },
  });

  let chromium;
  try {
    ({ chromium } = await import("playwright"));
  } catch (error) {
    console.error("Playwright is required for BuddyBoardGames autoplay.");
    console.error("Run with: npx --yes --package playwright node tools/buddyboardgames/autoplay.mjs --dry-run");
    throw error;
  }

  const browser = await chromium.launch({ headless: args.headed ? false : true });
  const page = await browser.newPage();

  try {
    await page.goto(url, { waitUntil: "domcontentloaded" });

    if (room || hasExplicitPlayer) {
      await joinRoom(page, player, room, {
        startGame: args.startGame,
        autoStartSolo: args.autoStartSolo,
      });
    }

    if (loop) {
      console.log(`joined: ${room || "(page default room)"}`);
      console.log(`player: ${playerLabel}`);
      console.log("status: autoplay-loop");
      await autoplayLoop(page, {
        oracleEndgame,
        agent,
        table,
        traceDir,
        pollMs: Number(args.pollMs || 1000),
        maxActions: args.maxActions ? Number(args.maxActions) : Infinity,
        restartGames: Boolean(args.restartGames),
        autoStartSolo: Boolean(args.autoStartSolo),
        stopController,
        session,
        dryRun,
      });
      return;
    }

    if (joinOnly) {
      console.log(`joined: ${room || "(page default room)"}`);
      console.log(`player: ${playerLabel}`);
      console.log("status: connected");
      if (args.keepOpen) {
        console.log("keeping browser open; press Ctrl-C to stop");
        await waitUntilStopped(page, stopController);
      }
      return;
    }

    await page.waitForTimeout(Number(args.waitMs || 500));
    const snapshot = await readSnapshot(page);
    if (args.probe) {
      console.log(JSON.stringify(snapshot.probe, null, 2));
    }

    const status = classifyProbeState(snapshot.probe);
    if (!status.ready) {
      for (const line of formatGuardStatus(status)) {
        console.log(line);
      }
      if (dryRun) {
        console.log("dry_run: true");
      }
      return;
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
  } finally {
    stopController.dispose();
    await browser.close();
  }
}

async function autoplayLoop(page, options) {
  let actions = 0;
  let games = 0;
  let lastSkip = "";
  let stopReason = "";
  let exitReason = "completed";

  while (actions < options.maxActions) {
    if (options.stopController?.requested()) {
      stopReason = options.stopController.reason();
      exitReason = stopReason;
      break;
    }

    const status = await readLoopStatus(page);
    if (status.ended) {
      const recorded = await recordCompletedGameScore(page, options.session);
      if (recorded !== null) {
        games = options.session.scores().length;
        console.log(`game_score: ${recorded} games=${games}`);
      }
      if (options.restartGames && await restartFinishedGame(page, options)) {
        lastSkip = "";
        console.log(`status: restarted games=${games}`);
        continue;
      }
      exitReason = "game-ended";
      console.log(`status: game-ended games=${games}`);
      break;
    }
    if (!status.ready) {
      const skip = `waiting: ${status.reason}`;
      if (skip !== lastSkip) {
        console.log(skip);
        lastSkip = skip;
      }
      await waitWithStop(page, options.pollMs, options.stopController);
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
      if (options.stopController?.requested()) {
        stopReason = options.stopController.reason();
        exitReason = stopReason;
        break;
      }
      console.log(`waiting: ${error.message}`);
    }

    await waitWithStop(page, options.pollMs, options.stopController);
  }

  if (!stopReason && options.stopController?.requested()) {
    stopReason = options.stopController.reason();
    exitReason = stopReason;
  }
  await recordCompletedGameScore(page, options.session).catch(() => null);

  if (actions >= options.maxActions) {
    exitReason = `max-actions reached (${options.maxActions})`;
    console.log(`status: ${exitReason}`);
  } else if (stopReason) {
    console.log(`status: stop-requested reason=${stopReason}`);
  }

  const summary = options.session.emitSummary({ reason: exitReason, forceChartAttempt: true });
  if (summary) {
    console.log(summary);
  }
}

async function readLoopStatus(page) {
  const probe = await readLoopProbe(page);
  if (probe.unavailable) {
    return { ready: false, ended: false, reason: probe.reason };
  }
  return classifyProbeState(probe);
}

async function readLoopProbe(page) {
  return page.evaluate(() => {
    if (typeof thisGame === "undefined" || !thisGame) {
      return { unavailable: true, reason: "thisGame unavailable" };
    }
    const game = thisGame;
    return {
      gameState: String(game.gameState || ""),
      meIdx: Number(game.meIdx),
      turnIdx: Number(game.turnIdx),
      isSpectator: Boolean(game.isSpectator),
      rollPending: Boolean(game.rollResultPending || window.yahtzeeRollDiceTimeoutId),
      turnsLeft: Number(game.turnsLeft),
    };
  });
}

function waitUntilStopped(page, stopController = createStopController()) {
  return new Promise((resolve) => {
    let done = false;
    let poll = null;
    const finish = () => {
      if (!done) {
        done = true;
        if (poll !== null) {
          clearInterval(poll);
        }
        resolve();
      }
    };
    if (stopController.requested()) {
      finish();
      return;
    }
    poll = setInterval(() => {
      if (stopController.requested()) {
        finish();
      }
    }, 100);
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
    return true;
  }
  return false;
}

async function restartFinishedGame(page, options) {
  if (await clickIfVisible(page, ".rematch-button")) {
    await page.waitForTimeout(800);
    return settleAfterRestart(page, options);
  }

  if (await clickIfVisible(page, "#gear")) {
    await page.waitForTimeout(250);
    if (await clickIfVisible(page, "#restart-button")) {
      await page.waitForTimeout(800);
      return settleAfterRestart(page, options);
    }
  }

  if (options.autoStartSolo && await isSoloLobby(page)) {
    return startLobbyGame(page);
  }

  return false;
}

async function settleAfterRestart(page, options) {
  if (options.autoStartSolo && await isSoloLobby(page)) {
    await startLobbyGame(page);
  }
  await page.waitForTimeout(options.pollMs);
  const status = await readLoopStatus(page);
  return !status.ended;
}

async function clickIfVisible(page, selector) {
  const target = page.locator(selector).first();
  if (!await target.isVisible().catch(() => false)) {
    return false;
  }
  await domClick(page, selector);
  return true;
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
      turnsLeft: Number(game.turnsLeft),
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

async function recordCompletedGameScore(page, session) {
  const finished = await readFinishedGameScore(page).catch(() => null);
  if (!finished) {
    return null;
  }
  return session.recordScore(finished.score, finished.signature);
}

async function readFinishedGameScore(page) {
  return page.evaluate(() => {
    if (typeof thisGame === "undefined" || !thisGame) {
      throw new Error("BuddyBoardGames thisGame is not available");
    }
    const game = thisGame;
    if (game.gameState !== "ENDED" && Number(game.turnsLeft) !== 0) {
      throw new Error("game not finished");
    }
    if (game.meIdx < 0 || !game.players || !game.players[game.meIdx]) {
      throw new Error("BuddyBoardGames current player is not readable");
    }

    const player = game.players[game.meIdx];
    const rows = player.score?.rows || {};
    const totalRow = rows[15];
    const fallbackTotal = Object.entries(rows)
      .filter(([key, row]) => key !== "15" && row && Number.isFinite(Number(row.value)))
      .reduce((sum, [, row]) => sum + Number(row.value || 0), 0);
    const score = Number(totalRow?.value);
    const normalizedScore = Number.isFinite(score) ? score : fallbackTotal;
    const signature = Object.entries(rows)
      .map(([key, row]) => `${key}:${Number(row?.value || 0)}:${row?.selected ? 1 : 0}`)
      .join(",");

    return {
      score: normalizedScore,
      signature,
    };
  });
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

async function waitWithStop(page, milliseconds, stopController) {
  if (!stopController) {
    await page.waitForTimeout(milliseconds);
    return;
  }
  const slice = Math.max(50, Math.min(250, milliseconds));
  let remaining = milliseconds;
  while (remaining > 0 && !stopController.requested()) {
    const delay = Math.min(slice, remaining);
    try {
      await page.waitForTimeout(delay);
    } catch (error) {
      if (stopController.requested()) {
        return;
      }
      throw error;
    }
    remaining -= delay;
  }
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

function isMainModule() {
  return Boolean(process.argv[1] && resolve(process.argv[1]) === modulePath);
}

function createStopController(options = {}) {
  let requested = false;
  let forceExit = false;
  let reason = "";
  let forceTimer = null;
  const handlers = new Map();
  const requestStop = (nextReason) => {
    if (!requested) {
      requested = true;
      reason = nextReason;
      options.onRequestStop?.(nextReason);
      forceTimer = setTimeout(() => {
        if (forceExit) {
          return;
        }
        forceExit = true;
        options.onForceStop?.(nextReason);
        process.exit(130);
      }, 4000);
      forceTimer.unref?.();
      return;
    }
    if (forceExit) {
      return;
    }
    forceExit = true;
    options.onForceStop?.(nextReason);
    process.exit(130);
  };

  for (const signal of ["SIGINT", "SIGTERM"]) {
    const handler = () => requestStop(signal);
    handlers.set(signal, handler);
    process.on(signal, handler);
  }

  return {
    requested: () => requested,
    reason: () => reason || "stop-requested",
    dispose: () => {
      if (forceTimer !== null) {
        clearTimeout(forceTimer);
      }
      for (const [signal, handler] of handlers.entries()) {
        process.off(signal, handler);
      }
      handlers.clear();
    },
  };
}

function createLoopSession() {
  const scores = [];
  const signatures = new Set();
  let summaryOutput = "";
  let chartPath = "";
  let chartUnavailableReason = "";

  return {
    recordScore(score, signature) {
      if (signatures.has(signature)) {
        return null;
      }
      signatures.add(signature);
      scores.push(score);
      return score;
    },
    scores: () => scores.slice(),
    getCachedSummary(options = {}) {
      if (summaryOutput) {
        return summaryOutput;
      }
      return formatScoreSummary(scores, {
        reason: options.reason,
        chartPath,
        chartUnavailableReason,
      });
    },
    emitSummary(options = {}) {
      if (summaryOutput) {
        return "";
      }
      if (options.forceChartAttempt && !chartPath && !chartUnavailableReason && scores.length) {
        const artifact = tryRenderScoreChart(scores);
        if (artifact.ok) {
          chartPath = artifact.path;
        } else {
          chartUnavailableReason = artifact.reason;
        }
      }
      summaryOutput = formatScoreSummary(scores, {
        reason: options.reason,
        chartPath,
        chartUnavailableReason,
      });
      return summaryOutput;
    },
  };
}

function tryRenderScoreChart(scores) {
  const environment = ensureMatplotlibEnvironment();
  if (!environment.ok) {
    return { ok: false, reason: environment.reason };
  }
  const python = environment.python;

  const reportsDir = resolve(repoRoot, "target/bbg-reports");
  mkdirSync(reportsDir, { recursive: true });
  const outputPath = join(reportsDir, `score-history-${timestampSlug()}.png`);
  const plotScript = `
import json
import os
import sys

try:
    import matplotlib
    matplotlib.use("Agg")
    import matplotlib.pyplot as plt
except Exception as exc:
    print(f"MATPLOTLIB_IMPORT_ERROR:{exc}", file=sys.stderr)
    sys.exit(3)

scores = json.loads(sys.argv[1])
output_path = sys.argv[2]
mean = sum(scores) / len(scores)
x_values = list(range(1, len(scores) + 1))

fig, ax = plt.subplots(figsize=(10, 4.8), dpi=160)
ax.plot(x_values, scores, color="#1f77b4", linewidth=2, marker="o", markersize=4)
ax.axhline(mean, color="#555555", linestyle="--", linewidth=1.5, label=f"Mean {mean:.2f}")
ax.set_title("BuddyBoardGames Score History")
ax.set_xlabel("Game")
ax.set_ylabel("Score")
ax.grid(True, axis="y", linestyle=":", linewidth=0.6, alpha=0.7)
ax.legend(loc="best")
fig.tight_layout()
fig.savefig(output_path)
print(output_path)
`;

  try {
    const stdout = execFileSync(
      python,
      ["-c", plotScript, JSON.stringify(scores), outputPath],
      { cwd: repoRoot, encoding: "utf8", stdio: ["ignore", "pipe", "pipe"] },
    ).trim();
    return { ok: true, path: stdout || outputPath };
  } catch (error) {
    const detail = String(error.stderr || error.message || "unknown python error").trim();
    const normalized = detail.replace(/\s+/g, " ");
    if (normalized.includes("MATPLOTLIB_IMPORT_ERROR")) {
      return { ok: false, reason: "matplotlib not installed" };
    }
    return { ok: false, reason: normalized };
  }
}

export function ensureMatplotlibEnvironment(options = {}) {
  const runner = options.execFileSync || execFileSync;
  const root = resolve(options.repoRoot || repoRoot);
  const existingEnvironments = discoverVirtualEnvironments(root);
  const pythonCandidates = discoverPythonExecutables(root, existingEnvironments);

  for (const candidate of pythonCandidates) {
    if (pythonHasModule(runner, candidate, "matplotlib", root)) {
      return { ok: true, python: candidate, source: "existing" };
    }
  }

  const existingVirtualEnv = existingEnvironments.find((envPath) =>
    pythonIsRunnable(runner, virtualEnvPython(envPath), root),
  );
  if (existingVirtualEnv) {
    const python = virtualEnvPython(existingVirtualEnv);
    const installed = installMatplotlib(runner, {
      python,
      repoRoot: root,
      preferUser: false,
    });
    if (!installed.ok) {
      return installed;
    }
    if (pythonHasModule(runner, python, "matplotlib", root)) {
      return { ok: true, python, source: "existing-venv" };
    }
    return { ok: false, reason: "matplotlib install completed but import still failed" };
  }

  if (commandAvailable(runner, "uv", root)) {
    const venvPath = resolve(root, ".venv");
    try {
      runner("uv", ["venv", venvPath], {
        cwd: root,
        encoding: "utf8",
        stdio: ["ignore", "pipe", "pipe"],
      });
    } catch (error) {
      return { ok: false, reason: `uv venv failed: ${normalizeCommandError(error)}` };
    }
    const python = virtualEnvPython(venvPath);
    const installed = installMatplotlib(runner, {
      python,
      repoRoot: root,
      preferUser: false,
    });
    if (!installed.ok) {
      return installed;
    }
    if (pythonHasModule(runner, python, "matplotlib", root)) {
      return { ok: true, python, source: "uv-created-venv" };
    }
    return { ok: false, reason: "matplotlib install completed but import still failed" };
  }

  const python = findPythonExecutable(runner, pythonCandidates, root);
  if (!python) {
    return { ok: false, reason: "python3 not available" };
  }
  const installed = installMatplotlib(runner, {
    python,
    repoRoot: root,
    preferUser: !pythonLooksVirtualEnv(python),
  });
  if (!installed.ok) {
    return installed;
  }
  if (pythonHasModule(runner, python, "matplotlib", root)) {
    return { ok: true, python, source: "pip-install" };
  }
  return { ok: false, reason: "matplotlib install completed but import still failed" };
}

function discoverVirtualEnvironments(root) {
  const candidates = [];
  if (process.env.VIRTUAL_ENV) {
    candidates.push(resolve(process.env.VIRTUAL_ENV));
  }
  for (const directory of [".venv", "venv", "env"]) {
    const envPath = resolve(root, directory);
    if (existsSync(envPath)) {
      candidates.push(envPath);
    }
  }
  return [...new Set(candidates)];
}

function discoverPythonExecutables(root, virtualEnvironments = discoverVirtualEnvironments(root)) {
  const candidates = [];
  for (const envPath of virtualEnvironments) {
    candidates.push(virtualEnvPython(envPath));
  }
  if (process.env.PYTHON) {
    candidates.push(process.env.PYTHON);
  }
  candidates.push("python3", "python");
  return [...new Set(candidates)];
}

function virtualEnvPython(envPath) {
  return process.platform === "win32"
    ? join(envPath, "Scripts", "python.exe")
    : join(envPath, "bin", "python");
}

function findPythonExecutable(runner, candidates, root) {
  for (const candidate of candidates) {
    if (!candidate) continue;
    if (pythonIsRunnable(runner, candidate, root)) {
      return candidate;
    }
  }
  return "";
}

function pythonIsRunnable(runner, python, root) {
  try {
    runner(python, ["-c", "import sys; print(sys.version_info[0])"], {
      cwd: root,
      encoding: "utf8",
      stdio: ["ignore", "pipe", "ignore"],
    });
    return true;
  } catch {
    return false;
  }
}

function pythonHasModule(runner, python, moduleName, root) {
  if (!pythonIsRunnable(runner, python, root)) {
    return false;
  }
  try {
    runner(python, ["-c", `import ${moduleName}`], {
      cwd: root,
      encoding: "utf8",
      stdio: ["ignore", "pipe", "pipe"],
    });
    return true;
  } catch {
    return false;
  }
}

function commandAvailable(runner, command, root) {
  try {
    runner(command, ["--version"], {
      cwd: root,
      encoding: "utf8",
      stdio: ["ignore", "pipe", "ignore"],
    });
    return true;
  } catch {
    return false;
  }
}

function installMatplotlib(runner, options) {
  if (commandAvailable(runner, "uv", options.repoRoot)) {
    try {
      runner("uv", ["pip", "install", "--python", options.python, "matplotlib"], {
        cwd: options.repoRoot,
        encoding: "utf8",
        stdio: ["ignore", "pipe", "pipe"],
      });
      return { ok: true };
    } catch (error) {
      return { ok: false, reason: `uv pip install matplotlib failed: ${normalizeCommandError(error)}` };
    }
  }

  const pipArgs = ["-m", "pip", "install"];
  if (options.preferUser) {
    pipArgs.push("--user");
  }
  pipArgs.push("matplotlib");
  try {
    runner(options.python, pipArgs, {
      cwd: options.repoRoot,
      encoding: "utf8",
      stdio: ["ignore", "pipe", "pipe"],
    });
    return { ok: true };
  } catch (error) {
    return { ok: false, reason: `pip install matplotlib failed: ${normalizeCommandError(error)}` };
  }
}

function pythonLooksVirtualEnv(python) {
  const normalized = python.replace(/\\/g, "/");
  return normalized.includes("/.venv/") || normalized.includes("/venv/") || normalized.includes("/env/");
}

function normalizeCommandError(error) {
  return String(error.stderr || error.message || "unknown command error").trim().replace(/\s+/g, " ");
}

function timestampSlug() {
  return new Date().toISOString().replace(/[:.]/g, "-");
}
