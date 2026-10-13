#!/usr/bin/env node
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { spawnSync } from 'node:child_process';

const identity = process.argv[2] || 'coordinator';
const args = process.argv.slice(3);
if (!/^(coordinator|worker-[1-9][0-9]*)$/.test(identity)) {
  console.error('invalid identity');
  process.exit(2);
}
const secretFile = path.join(os.homedir(), '.mnnz', 'crew-sync-local.env');
if (!fs.existsSync(secretFile)) throw new Error('CrewSync secret file missing');
const values = {};
for (const line of fs.readFileSync(secretFile, 'utf8').split(/\r?\n/)) {
  const i = line.indexOf('=');
  if (i > 0) values[line.slice(0, i)] = line.slice(i + 1);
}
const key = 'CREWSYNC_' + identity.toUpperCase().replaceAll('-', '_') + '_TOKEN';
const token = values[key];
if (!token) throw new Error('Token missing for identity ' + identity);
// Default host-native MCP transport avoids the Docker Desktop named-pipe hang.
// Docker is explicit diagnostics only; never replay uncertain writes across transports.
if (process.env.CREWSYNC_TRANSPORT !== 'docker') {
  const timeoutMs = Number(process.env.CREWSYNC_HTTP_TIMEOUT_MS ?? (args.includes('--all-pages') ? 90000 : 20000));
  if (!Number.isInteger(timeoutMs) || timeoutMs < 1000 || timeoutMs > 120000) {
    console.error('CREWSYNC_HTTP_CONFIG_ERROR');
    process.exit(2);
  }
  const r = spawnSync(process.execPath, [path.join(os.homedir(), '.mnnz', 'crew-sync-http-client.mjs'), ...args], {
    stdio: 'inherit', shell: false, timeout: timeoutMs, windowsHide: true,
    env: {...process.env, BUS_TOKEN: token, BUS_SESSION: process.env.CREWSYNC_SESSION ?? process.env.BUS_SESSION ?? ''}
  });
  if (r.error) {
    console.error('CREWSYNC_HTTP_RUNNER_FAILURE:', String(r.error.code ?? 'unknown'));
    process.exit(124);
  }
  process.exit(r.status ?? 1);
}
const dockerArgs = [
  'exec',
  '-e','BUS_URL=http://127.0.0.1:8787/mcp',
  '-e','BUS_TOKEN=' + token
];
if (process.env.CREWSYNC_SESSION) dockerArgs.push('-e','BUS_SESSION=' + process.env.CREWSYNC_SESSION);
dockerArgs.push('docker-bus-1','ai-crew-sync','client',...args);
const timeoutMs = Number(process.env.CREWSYNC_DOCKER_TIMEOUT_MS ?? 12000);
if (!Number.isInteger(timeoutMs) || timeoutMs < 1000 || timeoutMs > 120000) {
  console.error('CREWSYNC_CLIENT_CONFIG_ERROR: timeout must be 1000..120000ms');
  process.exit(2);
}
const r = spawnSync('docker', dockerArgs, {stdio:'inherit', shell:false, timeout:timeoutMs, windowsHide:true});
if (r.error?.code === 'ETIMEDOUT') {
  console.error('CREWSYNC_DOCKER_TIMEOUT: docker exec did not complete within ' + timeoutMs + 'ms; ownership state UNKNOWN, do not retry claim/complete blindly');
  process.exit(124);
}
if (r.error) {
  console.error('CREWSYNC_DOCKER_TRANSPORT_ERROR: ' + String(r.error.code ?? 'unknown'));
  process.exit(125);
}
process.exit(r.status ?? 1);
