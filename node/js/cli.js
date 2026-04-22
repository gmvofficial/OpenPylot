#!/usr/bin/env node
/**
 * CLI entry point for the Node.js package.
 *
 * Delegates to the native pylot binary, so all CLI
 * commands work identically to the Rust binary.
 */

const { execFileSync } = require('child_process');
const path = require('path');

function main() {
  const args = process.argv.slice(2);

  try {
    execFileSync('pylot', args, {
      stdio: 'inherit',
      env: process.env,
    });
  } catch (err) {
    if (err.status !== undefined) {
      process.exit(err.status);
    }

    console.error(
      'Error: pylot binary not found on PATH.\n' +
        'Install it first:\n' +
        '  curl -fsSL https://get.openpylot.dev/install.sh | sh\n' +
        '  # or: cargo install pylot'
    );
    process.exit(1);
  }
}

main();
