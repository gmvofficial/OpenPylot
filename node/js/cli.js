#!/usr/bin/env node
/**
 * CLI entry point for the Node.js package.
 *
 * Delegates to the native `pylot` binary so all CLI commands behave
 * identically to the Rust binary.
 *
 * IMPORTANT: this script is registered as the `pylot` bin of this npm
 * package. If we naively spawned `pylot` by name it would resolve back to
 * THIS script (when the npm bin is first on PATH) and recurse forever. So
 * we explicitly locate the real native executable and skip our own shim.
 */

const { execFileSync } = require('child_process');
const { existsSync, realpathSync, statSync } = require('fs');
const { join, delimiter } = require('path');
const os = require('os');

const isWindows = process.platform === 'win32';
const BINARY_NAME = isWindows ? 'pylot.exe' : 'pylot';

// The absolute path of this very script, resolved through symlinks, so we
// can make sure we never re-invoke ourselves.
let selfPath = null;
try {
  selfPath = realpathSync(__filename);
} catch (_) {
  selfPath = __filename;
}

function isRealExecutable(candidate) {
  try {
    if (!existsSync(candidate)) return false;
    const resolved = realpathSync(candidate);
    // Never treat our own shim (or anything inside this package) as the binary.
    if (resolved === selfPath) return false;
    if (resolved.endsWith('.js') || resolved.endsWith('.cjs')) return false;
    const st = statSync(resolved);
    return st.isFile();
  } catch (_) {
    return false;
  }
}

function resolveBinary() {
  // 1. Explicit override.
  if (process.env.PYLOT_BINARY && isRealExecutable(process.env.PYLOT_BINARY)) {
    return process.env.PYLOT_BINARY;
  }

  // 2. Common install locations (cargo, Homebrew, curl installer, system).
  const home = os.homedir();
  const candidates = [
    join(home, '.cargo', 'bin', BINARY_NAME),
    join(home, '.pylot', 'bin', BINARY_NAME),
    '/opt/homebrew/bin/' + BINARY_NAME,
    '/usr/local/bin/' + BINARY_NAME,
    '/usr/bin/' + BINARY_NAME,
  ];
  for (const c of candidates) {
    if (isRealExecutable(c)) return c;
  }

  // 3. Walk PATH, skipping our own shim.
  const pathDirs = (process.env.PATH || '').split(delimiter);
  for (const dir of pathDirs) {
    if (!dir) continue;
    const c = join(dir, BINARY_NAME);
    if (isRealExecutable(c)) return c;
  }

  return null;
}

function main() {
  const args = process.argv.slice(2);
  const binary = resolveBinary();

  if (!binary) {
    console.error(
      'Error: the native `pylot` binary was not found.\n' +
        'The npm package is a thin wrapper — install the binary first:\n' +
        '  cargo install openpylot\n' +
        '  # or: curl -fsSL https://raw.githubusercontent.com/gmvofficial/OpenPylot/main/install.sh | bash\n' +
        '\n' +
        'Or point PYLOT_BINARY at an existing pylot executable.'
    );
    process.exit(127);
  }

  try {
    execFileSync(binary, args, { stdio: 'inherit', env: process.env });
  } catch (err) {
    if (err && err.status !== undefined && err.status !== null) {
      process.exit(err.status);
    }
    console.error(`Error running ${binary}: ${err && err.message ? err.message : err}`);
    process.exit(1);
  }
}

main();
