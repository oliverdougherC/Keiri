import test from "node:test";
import assert from "node:assert/strict";
import { mkdtempSync, mkdirSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";

import {
  DEFAULT_PLAYER_NAME,
  classifyProbeState,
  ensureMatplotlibEnvironment,
  formatScoreSummary,
  formatGuardStatus,
  parseArgs,
} from "./autoplay.mjs";

test("parseArgs recognizes restart mode and keeps the Keiri default player name stable", () => {
  assert.equal(DEFAULT_PLAYER_NAME, "Keiri");
  assert.deepEqual(parseArgs(["--loop", "--restart-games", "--room=ladder-room"]), {
    loop: true,
    restartGames: true,
    room: "ladder-room",
  });
});

test("classifyProbeState treats the landing demo as waiting instead of playable", () => {
  const status = classifyProbeState({
    gameState: "DEMO",
    meIdx: 0,
    turnIdx: 0,
    isSpectator: false,
    rollPending: false,
    turnsLeft: 13,
  });

  assert.deepEqual(status, {
    ready: false,
    ended: false,
    reason: "gameState=DEMO",
  });
  assert.deepEqual(formatGuardStatus(status), [
    "status: waiting",
    "reason: gameState=DEMO",
  ]);
});

test("classifyProbeState keeps lobby joins guarded until the game starts", () => {
  const status = classifyProbeState({
    gameState: "LOBBY",
    meIdx: 0,
    turnIdx: 0,
    isSpectator: false,
    rollPending: false,
    turnsLeft: 13,
  });

  assert.deepEqual(status, {
    ready: false,
    ended: false,
    reason: "gameState=LOBBY",
  });
});

test("classifyProbeState rejects turns that are not actionable yet", () => {
  assert.deepEqual(
    classifyProbeState({
      gameState: "STARTED",
      meIdx: 1,
      turnIdx: 0,
      isSpectator: false,
      rollPending: false,
      turnsLeft: 12,
    }),
    {
      ready: false,
      ended: false,
      reason: "not my turn me=1 turn=0",
    },
  );

  assert.deepEqual(
    classifyProbeState({
      gameState: "STARTED",
      meIdx: 0,
      turnIdx: 0,
      isSpectator: false,
      rollPending: true,
      turnsLeft: 12,
    }),
    {
      ready: false,
      ended: false,
      reason: "roll/update pending",
    },
  );
});

test("classifyProbeState marks a started active turn as ready", () => {
  assert.deepEqual(
    classifyProbeState({
      gameState: "STARTED",
      meIdx: 0,
      turnIdx: 0,
      isSpectator: false,
      rollPending: false,
      turnsLeft: 12,
    }),
    {
      ready: true,
      ended: false,
      reason: "ready",
    },
  );
});

test("formatScoreSummary reports highest score and mean on stop", () => {
  const summary = formatScoreSummary([210, 275, 240], { reason: "SIGINT", height: 6 });

  assert.match(summary, /session stopped: SIGINT/);
  assert.match(summary, /completed_games: 3/);
  assert.match(summary, /highest_score: 275/);
  assert.match(summary, /mean_score: 241\.67/);
  assert.doesNotMatch(summary, /score graph:/);
});

test("formatScoreSummary prefers a rendered chart path when provided", () => {
  const summary = formatScoreSummary([210, 275, 240], {
    reason: "SIGTERM",
    chartPath: "/tmp/score-history.png",
  });

  assert.match(summary, /session stopped: SIGTERM/);
  assert.match(summary, /score_graph_png: \/tmp\/score-history\.png/);
  assert.doesNotMatch(summary, /score graph:/);
});

test("formatScoreSummary reports unavailable matplotlib charts without falling back to ANSI output", () => {
  const summary = formatScoreSummary([210, 275, 240], {
    reason: "game-ended",
    chartUnavailableReason: "matplotlib not installed",
  });

  assert.match(summary, /score_graph_png: unavailable \(matplotlib not installed\)/);
  assert.doesNotMatch(summary, /score graph:/);
  assert.doesNotMatch(summary, /chart_renderer:/);
});

test("ensureMatplotlibEnvironment creates a .venv with uv when needed", () => {
  const repoRoot = mkdtempSync(join(tmpdir(), "keiri-uv-"));
  const calls = [];
  let envPythonReady = false;
  const envPython = join(repoRoot, ".venv", "bin", "python");

  const fakeExec = (command, args) => {
    calls.push([command, ...args]);
    if (command === "uv" && args[0] === "--version") {
      return "uv 0.7.0";
    }
    if (command === "python3" && args.join(" ") === "-c import sys; print(sys.version_info[0])") {
      return "3";
    }
    if (command === "python3" && args.join(" ") === "-c import matplotlib") {
      throw Object.assign(new Error("missing"), { stderr: "ModuleNotFoundError" });
    }
    if (command === "uv" && args[0] === "venv") {
      mkdirSync(join(repoRoot, ".venv", "bin"), { recursive: true });
      return "";
    }
    if (command === envPython && args.join(" ") === "-c import sys; print(sys.version_info[0])") {
      return "3";
    }
    if (command === "uv" && args[0] === "pip" && args[1] === "install") {
      envPythonReady = true;
      return "installed";
    }
    if (command === envPython && args.join(" ") === "-c import matplotlib") {
      if (!envPythonReady) {
        throw Object.assign(new Error("missing"), { stderr: "ModuleNotFoundError" });
      }
      return "";
    }
    throw new Error(`Unexpected command: ${command} ${args.join(" ")}`);
  };

  const result = ensureMatplotlibEnvironment({ execFileSync: fakeExec, repoRoot });

  assert.deepEqual(result, {
    ok: true,
    python: envPython,
    source: "uv-created-venv",
  });
  assert.deepEqual(calls, [
    ["python3", "-c", "import sys; print(sys.version_info[0])"],
    ["python3", "-c", "import matplotlib"],
    ["python", "-c", "import sys; print(sys.version_info[0])"],
    ["uv", "--version"],
    ["uv", "venv", join(repoRoot, ".venv")],
    ["uv", "--version"],
    ["uv", "pip", "install", "--python", envPython, "matplotlib"],
    [envPython, "-c", "import sys; print(sys.version_info[0])"],
    [envPython, "-c", "import matplotlib"],
  ]);
});

test("ensureMatplotlibEnvironment falls back to pip when uv is unavailable", () => {
  const repoRoot = mkdtempSync(join(tmpdir(), "keiri-pip-"));
  const calls = [];
  let matplotlibInstalled = false;

  const fakeExec = (command, args) => {
    calls.push([command, ...args]);
    if (command === "python3" && args.join(" ") === "-c import sys; print(sys.version_info[0])") {
      return "3";
    }
    if (command === "python3" && args.join(" ") === "-c import matplotlib") {
      if (!matplotlibInstalled) {
        throw Object.assign(new Error("missing"), { stderr: "ModuleNotFoundError" });
      }
      return "";
    }
    if (command === "uv") {
      throw new Error("uv missing");
    }
    if (command === "python3" && args.join(" ") === "-m pip install --user matplotlib") {
      matplotlibInstalled = true;
      return "installed";
    }
    throw new Error(`Unexpected command: ${command} ${args.join(" ")}`);
  };

  const result = ensureMatplotlibEnvironment({ execFileSync: fakeExec, repoRoot });

  assert.deepEqual(result, {
    ok: true,
    python: "python3",
    source: "pip-install",
  });
  assert.deepEqual(calls, [
    ["python3", "-c", "import sys; print(sys.version_info[0])"],
    ["python3", "-c", "import matplotlib"],
    ["python", "-c", "import sys; print(sys.version_info[0])"],
    ["uv", "--version"],
    ["python3", "-c", "import sys; print(sys.version_info[0])"],
    ["uv", "--version"],
    ["python3", "-m", "pip", "install", "--user", "matplotlib"],
    ["python3", "-c", "import sys; print(sys.version_info[0])"],
    ["python3", "-c", "import matplotlib"],
  ]);
});
