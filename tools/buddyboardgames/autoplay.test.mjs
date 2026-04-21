import test from "node:test";
import assert from "node:assert/strict";

import {
  DEFAULT_PLAYER_NAME,
  classifyProbeState,
  formatScoreSummary,
  formatGuardStatus,
  parseArgs,
  renderScoreGraph,
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

test("renderScoreGraph draws score points with a dashed mean line", () => {
  const graph = renderScoreGraph([200, 260, 300], { height: 6, maxWidth: 10 });

  assert.match(graph, /score graph:/);
  assert.match(graph, /\*/);
  assert.match(graph, / mean/);
  assert.match(graph, /games 1\.\.3/);
});

test("formatScoreSummary reports highest score and mean on stop", () => {
  const summary = formatScoreSummary([210, 275, 240], { reason: "SIGINT", height: 6 });

  assert.match(summary, /session stopped: SIGINT/);
  assert.match(summary, /completed_games: 3/);
  assert.match(summary, /highest_score: 275/);
  assert.match(summary, /mean_score: 241\.67/);
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
