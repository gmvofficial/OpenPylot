#!/usr/bin/env node
/**
 * CLI entry point for the Node.js package.
 *
 * Delegates to the native gmv-agent binary, so all CLI
 * commands work identically to the Rust binary.
 */

const { execFileSync } = require('child_process');
const path = require('path');

function main() {
  const args = process.argv.slice(2);

  try {
    execFileSync('gmv-agent', args, {
      stdio: 'inherit',
      env: process.env,
    });
  } catch (err) {
    if (err.status !== undefined) {
      process.exit(err.status);
    }

    console.error(
      'Error: gmv-agent binary not found on PATH.\n' +
        'Install it first:\n' +
        '  curl -fsSL https://get.gmvagent.com/install.sh | sh\n' +
        '  # or: cargo install gmv-agent'
    );
    process.exit(1);
  }
}

main();
