import { randomUUID } from 'node:crypto';

export const DEFAULTS = Object.freeze({
  composeFile: 'Docker/docker-compose.yml',
  projectDirectory: '.',
  composeProject: 'docker',
  productionService: 'bus',
  productionPort: 18787,
  productionHost: '127.0.0.1',
  volume: 'docker_bus-data',
  cursorSecretEnv: 'TASK_CURSOR_SECRET',
  dashboardSecretEnv: 'BUS_DASHBOARD_SECRET',
  productionMigration: 19,
  candidateMigration: 21,
  rollbackMigration: 21,
  oldVersion: '0.7.3',
  mode: 'dry-run',
  canaryNetwork: 'local-mcp-bluegreen-private',
  canaryPort: 0,
  controllerLock: 'crewsync-local-mcp-bluegreen-rollout',
});

export class BlueGreenError extends Error {
  constructor(message, code = 'BLUEGREEN_FAILED', details = {}) {
    super(message);
    this.name = 'BlueGreenError';
    this.code = code;
    this.details = details;
  }
}

const nonEmpty = (value, name) => {
  if (typeof value !== 'string' || value.trim() === '') throw new BlueGreenError(`${name} is required`, 'INVALID_CONFIG');
  return value.trim();
};

function immutableImage(value, name) {
  const image = nonEmpty(value, name);
  if (!/@sha256:[a-f0-9]{16,}$/i.test(image)) throw new BlueGreenError(`${name} must be an immutable digest reference`, 'IMAGE_GATE');
  return image;
}

function imageEnvironment(image) {
  const at = image.indexOf('@');
  const tagged = at === -1 ? image : image.slice(0, at);
  const slash = tagged.lastIndexOf('/');
  const colon = tagged.indexOf(':', slash + 1);
  if (colon === -1) throw new BlueGreenError('immutable image must include a repository tag before its digest', 'IMAGE_GATE');
  return { BUS_IMAGE: tagged.slice(0, colon), BUS_VERSION: tagged.slice(colon + 1) + image.slice(at) };
}

export function normalizeConfig(input = {}) {
  const config = { ...DEFAULTS, ...input };
  if (!['dry-run', 'execute'].includes(config.mode)) throw new BlueGreenError('mode must be dry-run or execute', 'INVALID_CONFIG');
  config.candidateImage = immutableImage(config.candidateImage, 'candidateImage');
  config.rollbackImage = immutableImage(config.rollbackImage, 'rollbackImage');
  config.composeFile = nonEmpty(config.composeFile, 'composeFile');
  config.projectDirectory = nonEmpty(config.projectDirectory, 'projectDirectory');
  config.composeProject = nonEmpty(config.composeProject, 'composeProject');
  config.productionHost = nonEmpty(config.productionHost, 'productionHost');
  if (config.productionHost !== DEFAULTS.productionHost) throw new BlueGreenError('production host must remain loopback', 'SAFETY_GATE');
  config.productionPort = Number(config.productionPort);
  if (!Number.isInteger(config.productionPort) || config.productionPort !== DEFAULTS.productionPort) throw new BlueGreenError('production port must remain 18787', 'SAFETY_GATE');
  config.productionMigration = Number(config.productionMigration ?? DEFAULTS.productionMigration);
  config.candidateMigration = Number(config.candidateMigration ?? DEFAULTS.candidateMigration);
  config.rollbackMigration = Number(config.rollbackMigration ?? DEFAULTS.rollbackMigration);
  if (!Number.isInteger(config.productionMigration) || config.productionMigration < 1) throw new BlueGreenError('production migration is invalid', 'SCHEMA_GATE');
  if (!Number.isInteger(config.candidateMigration) || config.candidateMigration < DEFAULTS.candidateMigration) throw new BlueGreenError('candidate migration v21 or newer is required', 'SCHEMA_GATE');
  if (!Number.isInteger(config.rollbackMigration) || config.rollbackMigration < config.candidateMigration) throw new BlueGreenError('rollback migration is not compatible with candidate schema', 'SCHEMA_GATE');
  config.canaryPort = Number(config.canaryPort);
  if (!Number.isInteger(config.canaryPort) || config.canaryPort !== 0) throw new BlueGreenError('canaryPort must remain un-published at 0', 'SAFETY_GATE');
  config.canaryNetwork = nonEmpty(config.canaryNetwork, 'canaryNetwork');
  config.controllerLock = nonEmpty(config.controllerLock, 'controllerLock');
  config.acceptedSourceRevision = nonEmpty(config.acceptedSourceRevision, 'acceptedSourceRevision');
  config.candidateSourceRevision = nonEmpty(config.candidateSourceRevision, 'candidateSourceRevision');
  config.rollbackSourceRevision = nonEmpty(config.rollbackSourceRevision, 'rollbackSourceRevision');
  config.databaseEnvFile = nonEmpty(config.databaseEnvFile, 'databaseEnvFile');
  config.cursorSecretEnvFile = nonEmpty(config.cursorSecretEnvFile, 'cursorSecretEnvFile');
  config.backupPath = nonEmpty(config.backupPath, 'backupPath');
  config.allowCutover = config.allowCutover === true;
  config.releaseAuthority = config.releaseAuthority === true;
  config.controllerLockAcquired = config.controllerLockAcquired === true;
  return config;
}

function command(argv, options = {}) {
  return { argv: [...argv], options: { shell: false, ...options } };
}

function composeArgs(config) {
  return ['docker', 'compose', '--project-directory', config.projectDirectory, '--project-name', config.composeProject, '-f', config.composeFile];
}

function clientTasksCommand(target, options = {}) {
  const argv = ['docker', 'exec', target, '/usr/local/bin/ai-crew-sync', 'client', '--url', 'http://127.0.0.1:8787/mcp'];
  if (options.authenticated !== false) argv.push('--token', '${MCP_EPHEMERAL_TOKEN}');
  argv.push('--json', 'tasks', '--all-pages');
  if (options.search) argv.push('--search', options.search);
  return command(argv, { ...options, tokenEnv: options.authenticated === false ? undefined : 'MCP_EPHEMERAL_TOKEN' });
}

export function buildPreflightPlan(config) {
  const c = normalizeConfig(config);
  return [
    command([...composeArgs(c), 'config', '--quiet']),
    command(['docker', 'volume', 'inspect', c.volume]),
    command([...composeArgs(c), 'ps', '--status', 'running', c.productionService]),
    command(['docker', 'inspect', '--format', '{{json .Mounts}}', `${c.composeProject}-bus-1`]),
    command(['docker', 'inspect', '--format', '{{json .HostConfig.PortBindings}}', `${c.composeProject}-bus-1`]),
    command(['docker', 'exec', `${c.composeProject}-db-1`, 'pg_isready', '-U', 'bus', '-d', 'bus']),
    command(['docker', 'exec', `${c.composeProject}-db-1`, 'psql', '-U', 'bus', '-d', 'bus', '-tAc', "SELECT version FROM _sqlx_migrations ORDER BY version DESC LIMIT 1"]),
    command(['docker', 'inspect', '--format', '{{json .State.Health}}', `${c.composeProject}-bus-1`]),
  ];
}

export function buildBackupPlan(config) {
  const c = normalizeConfig(config);
  const stamp = config.backupId ?? 'bluegreen-backup';
  const containerPath = `/tmp/${stamp}.dump`;
  return [
    command(['docker', 'exec', `${c.composeProject}-db-1`, 'pg_dump', '-U', 'bus', '-d', 'bus', '--format=custom', '--file', containerPath]),
    command(['docker', 'cp', `${c.composeProject}-db-1:${containerPath}`, c.backupPath]),
    command(['certutil', '-hashfile', c.backupPath, 'SHA256']),
    command(['docker', 'cp', c.backupPath, `${c.composeProject}-db-1:/tmp/bluegreen-verified.dump`]),
    command(['docker', 'exec', `${c.composeProject}-db-1`, 'pg_restore', '--list', '/tmp/bluegreen-verified.dump']),
    command(['docker', 'exec', `${c.composeProject}-db-1`, 'psql', '-U', 'bus', '-d', 'bus', '-tAc', 'SELECT count(*) FROM tasks']),
    command(['docker', 'volume', 'inspect', c.volume]),
  ];
}

function requireClone(config) {
  const clone = config.isolatedDatabase;
  if (!clone || typeof clone !== 'object') throw new BlueGreenError('isolated migrated-v21 database capability is required', 'CANARY_GATE');
  nonEmpty(clone.network, 'isolatedDatabase.network');
  nonEmpty(clone.service, 'isolatedDatabase.service');
  return clone;
}

export function buildCanaryPlan(config, image = config.candidateImage) {
  const c = normalizeConfig(config);
  const clone = requireClone(c);
  const name = c.canaryName ?? `local-mcp-canary-${randomUUID()}`;
  return {
    name,
    commands: [
      command(['docker', 'network', 'create', '--internal', c.canaryNetwork]),
      command(['docker', 'network', 'connect', c.canaryNetwork, clone.service]),
      command(['docker', 'run', '-d', '--rm', '--name', name, '--network', c.canaryNetwork, '--network-alias', 'bus-candidate', '--env-file', c.databaseEnvFile, '--env-file', c.cursorSecretEnvFile, '-e', 'BUS_BIND=0.0.0.0:8787', '-e', 'BUS_PUBLICATION_WORKER=false', image, 'serve']),
      command(['docker', 'inspect', '--format', '{{json .State.Health}}', name]),
      clientTasksCommand(name, { proof: 'authenticated-mcp' }),
    ],
  };
}

export function buildProofPlan(config, canaryName = config.canaryName) {
  const c = normalizeConfig(config);
  const target = nonEmpty(canaryName, 'canaryName');
  return [
    command(['docker', 'inspect', '--format', '{{json .State.Health}}', target], { proof: 'health' }),
    clientTasksCommand(target, { proof: 'authenticated-mcp-tools-list-cursor-schema' }),
    clientTasksCommand(target, { search: '4053', proof: 'clone-4053-all-pages' }),
    clientTasksCommand(target, { authenticated: false, expectFailure: 'unauthorized', proof: 'negative-auth' }),
  ];
}

export function buildCleanupPlan(config, canaryName) {
  const c = normalizeConfig(config);
  const clone = requireClone(c);
  const target = nonEmpty(canaryName, 'canaryName');
  return [
    command(['docker', 'rm', '--force', target]),
    command(['docker', 'network', 'disconnect', c.canaryNetwork, clone.service]),
    command(['docker', 'network', 'rm', c.canaryNetwork]),
  ];
}

export function buildRollbackPlan(config, image = config.rollbackImage) {
  const c = normalizeConfig(config);
  return [
    command(['docker', 'image', 'inspect', image]),
    command([...composeArgs(c), 'up', '-d', '--no-deps', '--no-build', '--scale', `${c.productionService}=1`, c.productionService], { env: imageEnvironment(image), rollback: true }),
    command([...composeArgs(c), 'ps', '--status', 'running', c.productionService], { rollback: true }),
    command(['docker', 'inspect', '--format', '{{json .State.Health}}', `${c.composeProject}-bus-1`], { rollback: true, proof: 'health-readback' }),
    clientTasksCommand(`${c.composeProject}-bus-1`, { rollback: true, proof: 'rollback-mcp-readback' }),
  ];
}

export function buildCutoverPlan(config, image = config.candidateImage) {
  const c = normalizeConfig(config);
  return [
    command(['docker', 'image', 'inspect', image]),
    command([...composeArgs(c), 'up', '-d', '--no-deps', '--no-build', '--scale', `${c.productionService}=1`, c.productionService], { env: imageEnvironment(image), cutover: true }),
    command([...composeArgs(c), 'ps', '--status', 'running', c.productionService], { cutover: true }),
    command(['docker', 'inspect', '--format', '{{json .State.Health}}', `${c.composeProject}-bus-1`], { cutover: true, proof: 'health-readback' }),
    clientTasksCommand(`${c.composeProject}-bus-1`, { cutover: true, proof: 'post-cutover-mcp-readback' }),
  ];
}

function safeEvidence(result) {
  return { ok: result?.ok === true, exitCode: result?.exitCode ?? 0, classification: result?.classification ?? 'unclassified' };
}

function assertResult(result, step) {
  if (!result || result.ok !== true) throw new BlueGreenError(`${step} failed`, 'COMMAND_FAILED', { step, result: safeEvidence(result) });
}

async function runCommands(commands, runner, phase, evidence, validator) {
  for (const [index, item] of commands.entries()) {
    const result = await runner(item.argv, { ...item.options, phase, index });
    const safe = safeEvidence(result);
    evidence.push({ phase, index, argv: item.argv, result: safe });
    if (item.options.expectFailure) {
      if (result?.ok === true) throw new BlueGreenError(`${phase}[${index}] unexpectedly succeeded`, 'PROOF_GATE');
      if (validator) validator(result?.data, item, index);
      continue;
    }
    assertResult(result, `${phase}[${index}]`);
    if (validator) validator(result?.data, item, index);
  }
}

export function validatePreflight(snapshot, config) {
  const c = normalizeConfig(config);
  const v = snapshot ?? {};
  if (v.host !== c.productionHost || Number(v.port) !== c.productionPort) throw new BlueGreenError('production bind does not match loopback contract', 'SAFETY_GATE');
  if (v.volume !== c.volume || v.volumeIdentity !== c.volume) throw new BlueGreenError('database volume identity mismatch', 'SAFETY_GATE');
  if (Number(v.productionMigration) !== c.productionMigration) throw new BlueGreenError('existing production migration readback mismatch', 'SCHEMA_GATE');
  if (Number(v.candidateMigration) !== c.candidateMigration) throw new BlueGreenError('candidate migration is not v21', 'SCHEMA_GATE');
  if (Number(v.rollbackMigration) !== c.rollbackMigration || v.rollbackCompatible !== true) throw new BlueGreenError('rollback migration compatibility is not independently proven', 'SCHEMA_GATE');
  if (v.cursorSecretConfigured !== true || v.dashboardSecretPreserved !== true) throw new BlueGreenError('required secrets are not proven configured and preserved', 'AUTH_GATE');
  if (v.productionRunning !== true || v.databaseHealthy !== true || v.health !== 'ok') throw new BlueGreenError('production readiness is not proven', 'READINESS_GATE');
  if (v.existingStacks !== 1 || v.productionService !== c.productionService) throw new BlueGreenError('Compose project/service invariant failed', 'SAFETY_GATE');
  if (v.candidateSourceRevision !== c.acceptedSourceRevision) throw new BlueGreenError('candidate image source revision is not accepted release source', 'SOURCE_GATE');
  if (v.rollbackSourceRevision !== c.rollbackSourceRevision || v.rollbackCompatible !== true) throw new BlueGreenError('rollback source compatibility is not independently proven', 'SOURCE_GATE');
  if (v.candidateImageDigest !== c.candidateImage || v.rollbackImageDigest !== c.rollbackImage) throw new BlueGreenError('immutable image digest readback mismatch', 'IMAGE_GATE');
  return { ok: true };
}

export function validateRollbackReadback(readback = {}) {
  if (readback.health !== true || readback.mcp !== true || readback.compatible !== true) throw new BlueGreenError('rollback readiness readback is incomplete', 'ROLLBACK_GATE');
  return { ok: true };
}

export function validateProof(proof = {}) {
  const required = ['health', 'authenticatedMcp', 'schemaCursor', 'allPages', 'uniqueTasks', 'expectedTaskCount', 'clone4053', 'wrongMethod4053', 'tokenRevoked'];
  for (const key of required) if (proof[key] !== true) throw new BlueGreenError(`${key} proof failed`, key === 'health' ? 'CANARY_GATE' : 'PROOF_GATE');
  if (proof.pageCount !== undefined && (!Number.isInteger(proof.pageCount) || proof.pageCount <= 0)) throw new BlueGreenError('all-pages count is invalid', 'PROOF_GATE');
  if (proof.uniqueKeyCount !== undefined && proof.uniqueKeyCount !== proof.pageCount) throw new BlueGreenError('all-pages traversal contains duplicate or missing keys', 'PROOF_GATE');
  if (proof.exhausted !== undefined && proof.exhausted !== true) throw new BlueGreenError('all-pages traversal did not exhaust', 'PROOF_GATE');
  if (proof.hasMore !== undefined && proof.hasMore !== false) throw new BlueGreenError('all-pages traversal still has more pages', 'PROOF_GATE');
  return { ok: true };
}

function validateAuthority(config, dependencies, runner) {
  if (config.mode !== 'execute') return;
  if (typeof runner !== 'function' || runner.simulated === true) throw new BlueGreenError('execute mode requires a real injected argv runner', 'EXECUTION_GATE');
  if (config.allowCutover && (!config.releaseAuthority || !config.controllerLockAcquired)) throw new BlueGreenError('cutover requires release authority and controller lock', 'AUTHORITY_GATE');
  if (config.allowCutover && dependencies.preflight === undefined) throw new BlueGreenError('cutover requires structured preflight readback', 'READBACK_GATE');
  if (config.allowCutover && dependencies.proof === undefined) throw new BlueGreenError('cutover requires structured proof readback', 'READBACK_GATE');
  if (config.allowCutover && dependencies.rollback === undefined) throw new BlueGreenError('cutover requires rollback readiness readback', 'READBACK_GATE');
}

export async function runBlueGreen(input = {}, dependencies = {}) {
  let config;
  try {
    config = normalizeConfig(input);
  } catch (error) {
    return { status: 'BLOCKED', phases: [], cutover: 'not-authorized', error: { code: error.code, message: error.message } };
  }
  const runner = dependencies.runner;
  const evidence = [];
  const result = { mode: config.mode, config: { ...config, databaseEnvFile: '[redacted-path]', cursorSecretEnvFile: '[redacted-path]' }, phases: [], evidence, cutover: 'not-authorized' };
  let canaryName;
  try {
    validateAuthority(config, dependencies, runner);
    if (config.mode === 'execute') requireClone(config);
    result.phases.push('preflight');
    if (dependencies.preflight) validatePreflight(dependencies.preflight, config);
    else if (config.mode === 'execute') throw new BlueGreenError('structured preflight readback is required', 'READBACK_GATE');
    result.phases.push('backup');
    if (config.mode === 'execute') await runCommands(buildBackupPlan(config), runner, 'backup', evidence);
    result.phases.push('canary');
    const canary = buildCanaryPlan(config);
    canaryName = canary.name;
    result.canary = { name: canary.name, commands: canary.commands.map((item) => ({ argv: item.argv, options: { ...item.options, env: undefined } })) };
    if (config.mode === 'execute') await runCommands(canary.commands, runner, 'canary', evidence);
    result.phases.push('proof');
    if (dependencies.proof) {
      validateProof(dependencies.proof);
      if (config.mode === 'execute') await runCommands([command(['bluegreen', 'proof-fixture'], { fixture: true })], runner, 'proof', evidence);
    } else if (config.mode === 'execute') {
      await runCommands(buildProofPlan(config, canary.name), runner, 'proof', evidence);
    }
    result.phases.push('rollback-readiness');
    result.rollback = buildRollbackPlan(config);
    if (config.mode === 'execute' && config.allowCutover === true) validateRollbackReadback(dependencies.rollback);
    if (config.mode === 'execute' && config.allowCutover !== true) return { ...result, status: 'READY_NOT_CUTOVER', reason: 'release authority is required for production cutover' };
    if (config.allowCutover === true) {
      result.phases.push('cutover');
      await runCommands(buildCutoverPlan(config), runner, 'cutover', evidence);
      result.cutover = 'executed-and-read-back';
      result.status = 'CUTOVER_EXECUTED';
    } else {
      result.status = config.mode === 'dry-run' ? 'DRY_RUN' : 'READY_NOT_CUTOVER';
    }
    return result;
  } catch (error) {
    result.status = 'BLOCKED';
    result.error = { name: error.name, code: error.code ?? 'ERROR', message: error.message, details: error.details };
    return result;
  } finally {
    if (config?.mode === 'execute' && canaryName && runner && dependencies.cleanup !== false) {
      try { await runCommands(buildCleanupPlan(config, canaryName), runner, 'cleanup', evidence); } catch { /* cleanup evidence is best effort and never authorizes cutover */ }
    }
  }
}

export function formatReport(result) {
  return JSON.stringify({ outcome: result.status === 'BLOCKED' ? 'BLOCKED' : 'CANDIDATE', status: result.status, phases: result.phases, cutover: result.cutover, error: result.error ?? null }, null, 2);
}

if (import.meta.url === `file://${process.argv[1]}`) {
  const result = await runBlueGreen({
    candidateImage: process.env.CANDIDATE_IMAGE ?? 'local/ai-crew-sync:search-candidate@sha256:0000000000000000',
    rollbackImage: process.env.ROLLBACK_IMAGE ?? 'local/ai-crew-sync:rollback-v21@sha256:0000000000000000',
    acceptedSourceRevision: process.env.ACCEPTED_SOURCE_REVISION ?? 'unset',
    candidateSourceRevision: process.env.CANDIDATE_SOURCE_REVISION ?? 'unset',
    rollbackSourceRevision: process.env.ROLLBACK_SOURCE_REVISION ?? 'unset',
    databaseEnvFile: process.env.DATABASE_ENV_FILE ?? '.env.clone.redacted',
    cursorSecretEnvFile: process.env.CURSOR_SECRET_ENV_FILE ?? '.env.cursor.redacted',
    backupPath: process.env.BLUEGREEN_BACKUP_PATH ?? './bluegreen-backup.dump',
    mode: process.env.BLUEGREEN_MODE ?? 'dry-run',
  });
  process.stdout.write(`${formatReport(result)}\n`);
  if (result.status === 'BLOCKED') process.exitCode = 2;
}
