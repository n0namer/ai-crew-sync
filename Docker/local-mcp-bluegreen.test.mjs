import test from 'node:test';
import assert from 'node:assert/strict';

import {
  BlueGreenError,
  DEFAULTS,
  buildBackupPlan,
  buildCanaryPlan,
  buildCleanupPlan,
  buildCutoverPlan,
  buildPreflightPlan,
  buildProofPlan,
  buildRollbackPlan,
  normalizeConfig,
  runBlueGreen,
  validatePreflight,
  validateProof,
  validateRollbackReadback,
} from './local-mcp-bluegreen.mjs';

const candidateImage = 'local/ai-crew-sync:search-final@sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa';
const rollbackImage = 'local/ai-crew-sync:rollback-v21@sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb';
const config = {
  candidateImage,
  rollbackImage,
  acceptedSourceRevision: '52cb682',
  candidateSourceRevision: '52cb682',
  rollbackSourceRevision: '8897d7a',
  databaseEnvFile: '.env.clone.redacted',
  cursorSecretEnvFile: '.env.cursor.redacted',
  backupPath: './artifacts/bluegreen-v21.dump',
  isolatedDatabase: { network: 'clone-db-network', service: 'search-v21-db' },
};

const ready = {
  host: '127.0.0.1',
  port: 18787,
  volume: 'docker_bus-data',
  volumeIdentity: 'docker_bus-data',
  productionMigration: 19,
  candidateMigration: 21,
  rollbackMigration: 21,
  migration: 21,
  cursorSecretConfigured: true,
  dashboardSecretPreserved: true,
  productionRunning: true,
  databaseHealthy: true,
  health: 'ok',
  existingStacks: 1,
  productionService: 'bus',
  candidateSourceRevision: '52cb682',
  rollbackSourceRevision: '8897d7a',
  rollbackCompatible: true,
  candidateImageDigest: candidateImage,
  rollbackImageDigest: rollbackImage,
};

const proof = {
  health: true,
  authenticatedMcp: true,
  schemaCursor: true,
  allPages: true,
  uniqueTasks: true,
  expectedTaskCount: true,
  clone4053: true,
  wrongMethod4053: true,
  tokenRevoked: true,
};

const runner = async () => ({ ok: true, data: {} });
const rollback = { health: true, mcp: true, compatible: true };

test('defaults to safe dry-run and never invokes a runner', async () => {
  let calls = 0;
  const result = await runBlueGreen(config, {
    runner: async () => { calls += 1; return { ok: true }; },
    preflight: ready,
    proof,
  });
  assert.equal(result.status, 'DRY_RUN');
  assert.equal(result.cutover, 'not-authorized');
  assert.equal(calls, 0);
  assert.deepEqual(result.phases, ['preflight', 'backup', 'canary', 'proof', 'rollback-readiness']);
});

test('requires immutable digests, accepted source parity, and loopback production', () => {
  assert.throws(() => normalizeConfig({ ...config, candidateImage: 'local/candidate:latest' }), BlueGreenError);
  assert.throws(() => normalizeConfig({ ...config, productionHost: '0.0.0.0' }), /loopback/);
  assert.throws(() => validatePreflight({ ...ready, candidateSourceRevision: '45e33ed' }, config), /source revision/);
});

test('preflight fails closed for auth, schema, volume, health, and duplicate-stack mismatches', () => {
  for (const change of [
    { cursorSecretConfigured: false },
    { dashboardSecretPreserved: false },
    { productionMigration: 20 },
    { candidateMigration: 20 },
    { rollbackMigration: 20 },
    { rollbackCompatible: false },
    { volumeIdentity: 'other-volume' },
    { health: 'degraded' },
    { existingStacks: 2 },
  ]) assert.throws(() => validatePreflight({ ...ready, ...change }, config), BlueGreenError);
});

test('backup is durable on the host and includes checksum, restore catalog, task count, and volume identity', () => {
  const commands = buildBackupPlan(config).map(({ argv }) => argv);
  assert.ok(commands.some((argv) => argv[0] === 'docker' && argv[1] === 'cp' && argv.includes(config.backupPath)));
  assert.ok(commands.some((argv) => argv[0] === 'certutil' && argv.includes('-hashfile') && argv.includes(config.backupPath)));
  assert.ok(commands.some((argv) => argv[0] === 'docker' && argv.includes('pg_restore') && argv.includes('--list')));
  assert.ok(commands.some((argv) => argv.includes('docker_bus-data')));
});

test('canary is detached, private, clone-connected, unpublished, and does not expose credentials', () => {
  const commands = buildCanaryPlan(config, candidateImage).commands;
  const run = commands.find(({ argv }) => argv[1] === 'run');
  assert.ok(run.argv.includes('-d'));
  assert.ok(run.argv.includes('--network') && run.argv.includes(DEFAULTS.canaryNetwork));
  assert.ok(run.argv.includes('--env-file'));
  assert.ok(!run.argv.includes('-p'));
  assert.ok(run.argv.includes('BUS_PUBLICATION_WORKER=false'));
  assert.ok(!run.argv.some((value) => value.includes('postgres://') || value.includes('SECRET=')));
  const canaryClient = commands.find(({ argv }) => argv[3] === '/usr/local/bin/ai-crew-sync');
  assert.equal(canaryClient.argv[4], 'client');
  assert.equal(canaryClient.argv[5], '--url');
  assert.equal(canaryClient.argv[7], '--token');
  assert.equal(canaryClient.argv[9], '--json');
  assert.equal(canaryClient.argv[10], 'tasks');
  assert.equal(canaryClient.argv[11], '--all-pages');
  assert.ok(commands.some(({ argv }) => argv.includes('network') && argv.includes('connect') && argv.includes('search-v21-db')));
});

test('proof contract is fail-closed and includes token revocation', () => {
  assert.deepEqual(validateProof({ ...proof, pageCount: 4053, uniqueKeyCount: 4053, exhausted: true, hasMore: false }), { ok: true });
  assert.deepEqual(validateRollbackReadback(rollback), { ok: true });
  assert.throws(() => validateRollbackReadback({ ...rollback, compatible: false }), BlueGreenError);
  for (const key of Object.keys(proof)) assert.throws(() => validateProof({ ...proof, [key]: false }), BlueGreenError);
  const proofPlans = buildProofPlan(config, 'fixed-canary');
  const proofCommands = proofPlans.map(({ argv }) => argv);
  const clientCommands = proofCommands.filter((argv) => argv[3] === '/usr/local/bin/ai-crew-sync');
  assert.equal(clientCommands.length, 3);
  for (const argv of clientCommands) {
    assert.equal(argv[4], 'client');
    assert.equal(argv[5], '--url');
    assert.equal(argv[6], 'http://127.0.0.1:8787/mcp');
    assert.equal(argv[argv.indexOf('--json') + 1], 'tasks');
    assert.ok(argv.includes('--all-pages'));
  }
  const authenticated = clientCommands.find((argv) => argv.includes('--token'));
  assert.equal(authenticated[7], '--token');
  assert.equal(authenticated[8], '${MCP_EPHEMERAL_TOKEN}');
  assert.ok(proofCommands.some((argv) => argv.includes('4053')));
  assert.ok(proofPlans.some(({ options }) => options.expectFailure === 'unauthorized'));
  assert.ok(!proofCommands.some((argv) => argv.includes('ai-crew-sync', 4) || argv.includes('curl') || argv.includes('--expect-unique-keys')));
});

test('missing clone capability blocks execute mode before any side effect', async () => {
  const calls = [];
  const result = await runBlueGreen({ ...config, mode: 'execute', isolatedDatabase: undefined }, {
    preflight: ready,
    proof,
    runner: async (argv) => { calls.push(argv); return { ok: true }; },
  });
  assert.equal(result.status, 'BLOCKED');
  assert.equal(result.error.code, 'CANARY_GATE');
  assert.equal(calls.length, 0);
});

test('execute mode records backup, canary, proof, cleanup, and never cuts over without authority', async () => {
  const calls = [];
  const result = await runBlueGreen({ ...config, mode: 'execute', canaryName: 'fixed-canary' }, {
    preflight: ready,
    proof,
    runner: async (argv, options) => { calls.push({ argv, options }); return { ok: true, data: {} }; },
  });
  assert.equal(result.status, 'READY_NOT_CUTOVER');
  assert.equal(result.cutover, 'not-authorized');
  assert.ok(calls.some(({ options }) => options.phase === 'backup'));
  assert.ok(calls.some(({ options }) => options.phase === 'canary'));
  assert.ok(calls.some(({ options }) => options.phase === 'proof'));
  assert.ok(calls.some(({ options }) => options.phase === 'cleanup'));
  assert.ok(!calls.some(({ options }) => options.phase === 'cutover'));
});

test('authorized execute performs only bus Compose cutover and post-cutover MCP readback', async () => {
  const calls = [];
  const result = await runBlueGreen({ ...config, mode: 'execute', allowCutover: true, releaseAuthority: true, controllerLockAcquired: true, canaryName: 'fixed-canary' }, {
    preflight: ready,
    proof,
    rollback,
    runner: async (argv, options) => { calls.push({ argv, options }); return { ok: true, data: {} }; },
  });
  assert.equal(result.status, 'CUTOVER_EXECUTED');
  assert.equal(result.cutover, 'executed-and-read-back');
  const cutover = calls.filter(({ options }) => options.phase === 'cutover');
  assert.ok(cutover.some(({ argv }) => argv.includes('--project-directory') && argv.includes('.') && argv.includes('--project-name') && argv.includes('docker')));
  assert.ok(cutover.some(({ argv }) => argv.includes('--no-deps') && argv.includes('--no-build') && argv.includes('bus')));
  assert.ok(cutover.every(({ argv }) => !argv.includes('db') && !argv.includes('nats')));
  assert.ok(cutover.some(({ options }) => options.proof === 'post-cutover-mcp-readback'));
});

test('authority and structured readbacks are mandatory for real execute', async () => {
  const result = await runBlueGreen({ ...config, mode: 'execute', allowCutover: true, releaseAuthority: false, controllerLockAcquired: false }, {
    runner,
    preflight: ready,
    proof,
  });
  assert.equal(result.status, 'BLOCKED');
  assert.equal(result.error.code, 'AUTHORITY_GATE');
});

test('rollback is exact-project, bus-only, no-build, one-replica, and digest-pinned', () => {
  const commands = buildRollbackPlan(config);
  assert.ok(commands.some(({ argv }) => argv.includes('--project-directory') && argv.includes('--project-name') && argv.includes('docker')));
  assert.ok(commands.some(({ argv }) => argv.includes('--no-deps') && argv.includes('--no-build') && argv.includes('bus')));
  assert.ok(commands.some(({ argv }) => argv.includes('--scale') && argv.includes('bus=1')));
  assert.ok(commands.every(({ options }) => options.shell === false));
  assert.ok(commands.every(({ argv }) => !argv.includes('0.7.3')));
  assert.ok(buildCleanupPlan(config, 'fixed-canary').every(({ options }) => options.shell === false));
});
