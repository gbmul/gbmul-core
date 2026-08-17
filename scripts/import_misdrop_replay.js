#!/usr/bin/env node
/**
 * Phase 4 — import browser misdrop replays into fixture library (manifest v2).
 *
 * Usage:
 *   node scripts/import_misdrop_replay.js <replay.json> --id misdrop_l_spin_r16_c5 --claims misdrop_detection,planner_reachability
 *   node scripts/import_misdrop_replay.js --stdin --id auto --claims executor_path
 *
 * Fixture ids must name capability + board target — NOT piece pairing.
 * Use --claims to declare what this case tests (required).
 *
 * Writes:
 *   gbmul-core/tests/fixtures/misdrop/<id>_state.b64
 *   gbmul-core/tests/fixtures/misdrop/<id>_meta.json
 *   gbmul-core/tests/fixtures/misdrop/<id>_replay.json
 *
 * board_id: run `cargo test -p gbmul-core manifest_board_id` after import to fill in.
 */

const fs = require('fs');
const path = require('path');
const FIXTURE_DIR = path.join(__dirname, '..', 'gbmul-core', 'tests', 'fixtures', 'misdrop');
const PIECE_NAMES = ['I', 'O', 'T', 'S', 'Z', 'L', 'J'];

const CLAIM_KINDS = new Set([
  'planner_reachability',
  'executor_path',
  'srs_negative',
  'misdrop_detection',
  'spawn_capture',
  'auxiliary_board',
]);

const KNOWLEDGE_LEVELS = new Set(['proven', 'documented_gap', 'untested']);

function usage() {
  console.error(`Usage:
  node scripts/import_misdrop_replay.js <replay.json> --id <capability_id> --claims <kind>[,<kind>...]
  node scripts/import_misdrop_replay.js --stdin --id auto --claims misdrop_detection

  --claims is required. Piece pairing goes in pair_label only.
  After import, add board_id from: cargo test -p gbmul-core manifest_board_id_for -- --nocapture`);
  process.exit(1);
}

function parseArgs(argv) {
  const args = { id: null, stdin: false, file: null, claims: null, knowledge: 'untested' };
  for (let i = 2; i < argv.length; i++) {
    if (argv[i] === '--stdin') args.stdin = true;
    else if (argv[i] === '--id') args.id = argv[++i];
    else if (argv[i] === '--claims') args.claims = argv[++i];
    else if (argv[i] === '--knowledge') args.knowledge = argv[++i];
    else if (!argv[i].startsWith('-')) args.file = argv[i];
  }
  return args;
}

function loadEntry(raw) {
  let entry = typeof raw === 'string' ? JSON.parse(raw) : raw;
  if (typeof entry === 'string') entry = JSON.parse(entry);
  if (Array.isArray(entry)) entry = entry[entry.length - 1];
  if (entry.replay) entry = entry.replay;
  return entry;
}

function slugFromLabel(label) {
  console.error(
    'WARN: --id auto derives from pair_label; prefer explicit capability id, e.g. misdrop_l_spin_r16_c5'
  );
  return (label || 'misdrop')
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, '_')
    .replace(/^_|_$/g, '')
    .slice(0, 40) || 'misdrop_latest';
}

function pieceName(pt) {
  return PIECE_NAMES[pt] || '?';
}

function formatMisdropTs(ms) {
  const d = new Date(ms);
  const p = (n) => String(n).padStart(2, '0');
  return `${d.getFullYear()}${p(d.getMonth() + 1)}${p(d.getDate())}-${p(d.getHours())}${p(d.getMinutes())}${p(d.getSeconds())}`;
}

function pathTerminalMtype(path) {
  if (!path?.length) return 'normal';
  const last = [...path].reverse().find((a) => a !== 'D');
  if (last === 'CW' || last === 'CCW') return 'spin';
  if (last === 'L' || last === 'R') return 'tuck';
  return 'normal';
}

function applyMoveTypeFromPath(meta) {
  const m = meta?.misdrop;
  if (!m?.path?.length || m.move_type) return;
  m.move_type = pathTerminalMtype(m.path);
}

function pairLabelFromEntry(entry) {
  if (entry.label && !/#\d+\s*$/.test(entry.label)) return entry.label;
  const meta = entry.meta || entry;
  const cur = pieceName(meta.current_piece?.piece_type);
  const nxt = pieceName(meta.next_piece?.piece_type);
  const ts = entry.ts || Date.now();
  return `${cur}→${nxt} ${formatMisdropTs(ts)}`;
}

function boardIdFromStateB64(stateB64) {
  // Locked-field hash must match Rust board_id_from_ram — import leaves null; use cargo test.
  return null;
}

function buildClaims(claimStr, knowledge) {
  if (!KNOWLEDGE_LEVELS.has(knowledge)) {
    console.error(`Invalid --knowledge ${knowledge}`);
    process.exit(1);
  }
  return claimStr.split(',').map((kind) => {
    kind = kind.trim();
    if (!CLAIM_KINDS.has(kind)) {
      console.error(`Unknown claim kind: ${kind}`);
      process.exit(1);
    }
    const claim = { kind, knowledge, enforced_in_ci: false };
    if (kind === 'misdrop_detection' || kind === 'executor_path') {
      claim.notes = 'Imported — set knowledge/baseline after classifying';
    }
    return claim;
  });
}

async function main() {
  const args = parseArgs(process.argv);
  if (!args.claims) usage();

  let raw;
  if (args.stdin) {
    raw = await new Promise((res, rej) => {
      let buf = '';
      process.stdin.setEncoding('utf8');
      process.stdin.on('data', (c) => (buf += c));
      process.stdin.on('end', () => res(buf));
      process.stdin.on('error', rej);
    });
  } else if (args.file) {
    raw = fs.readFileSync(args.file, 'utf8');
  } else {
    usage();
  }

  const entry = loadEntry(raw.trim());
  const meta = entry.meta || entry;
  applyMoveTypeFromPath(meta);
  if (entry.meta) applyMoveTypeFromPath(entry.meta);
  if (entry.type !== undefined && meta.misdrop?.move_type) entry.type = meta.misdrop.move_type;
  const state = entry.state || entry.savestate;
  if (!state) {
    console.error('No state/savestate in replay');
    process.exit(1);
  }

  const id = args.id === 'auto' || !args.id ? slugFromLabel(entry.label) : args.id;
  if (id.includes('_to_')) {
    console.error('ERROR: fixture id must not use piece-pair slug. Use capability name.');
    process.exit(1);
  }

  fs.mkdirSync(FIXTURE_DIR, { recursive: true });

  const b64Path = path.join(FIXTURE_DIR, `${id}_state.b64`);
  const metaPath = path.join(FIXTURE_DIR, `${id}_meta.json`);
  const replayPath = path.join(FIXTURE_DIR, `${id}_replay.json`);

  fs.writeFileSync(b64Path, typeof state === 'string' ? state : JSON.stringify(state));
  fs.writeFileSync(metaPath, JSON.stringify(meta, null, 2) + '\n');
  fs.writeFileSync(replayPath, JSON.stringify(entry, null, 2) + '\n');

  const m = meta.misdrop || {};
  const cur = meta.current_piece || {};
  const next = meta.next_piece || {};
  const pathArr = m.path || [];
  const claims = buildClaims(args.claims, args.knowledge);

  const snippet = {
    id,
    b64: `${id}_state.b64`,
    meta: `${id}_meta.json`,
    board_id: boardIdFromStateB64(typeof state === 'string' ? state : null),
    legacy_id: null,
    pair_label: pairLabelFromEntry(entry),
    piece: pieceName(cur.piece_type),
    next: pieceName(next.piece_type),
    spawn:
      cur.spawn_col != null
        ? { row: -2, col: cur.spawn_col, rot: cur.rot ?? 0 }
        : null,
    class: m.move_type || 'unknown',
    recorded_path: pathArr.length ? pathArr : null,
    want_lock:
      m.wanted_col != null
        ? {
            row: m.wanted_row,
            col: m.wanted_col,
            rot: m.wanted_rot,
          }
        : null,
    generalizes: false,
    claims,
    notes: `Imported ${new Date().toISOString().slice(0, 10)} — set board_id via manifest_board_id_for test`,
  };

  console.log('Wrote:');
  console.log(' ', b64Path);
  console.log(' ', metaPath);
  console.log(' ', replayPath);
  console.log('\nmanifest.json snippet (v2):\n');
  console.log(JSON.stringify(snippet, null, 2));
  console.log(
    '\nNext: cargo test -p gbmul-core manifest_board_id_for -- --nocapture'
  );
}

main().catch((e) => {
  console.error(e);
  process.exit(1);
});