import { describe, it } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync, existsSync } from 'node:fs';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';

const __dirname = dirname(fileURLToPath(import.meta.url));
const nodeDir = join(__dirname, '..', '..', '..', 'node');

// ── TypeScript declarations integrity (generated index.d.ts) ─────────

describe('TypeScript declarations', () => {
  const indexTs = readFileSync(join(nodeDir, 'index.d.ts'), 'utf-8');

  it('should declare Config interface with llmProvider', () => {
    assert.ok(indexTs.includes('llmProvider: string'));
  });

  it('should declare Config interface with llmModel', () => {
    assert.ok(indexTs.includes('llmModel: string'));
  });

  it('should declare optional openaiApiKey', () => {
    assert.ok(indexTs.includes('openaiApiKey?: string'));
  });

  it('should declare optional anthropicApiKey', () => {
    assert.ok(indexTs.includes('anthropicApiKey?: string'));
  });

  it('should declare optional googleCredentialsFile', () => {
    assert.ok(indexTs.includes('googleCredentialsFile?: string'));
  });

  it('should declare optional telegramBotToken', () => {
    assert.ok(indexTs.includes('telegramBotToken?: string'));
  });

  it('should declare optional telegramChatId', () => {
    assert.ok(indexTs.includes('telegramChatId?: string'));
  });

  it('should declare PylotAgent class', () => {
    assert.ok(indexTs.includes('class PylotAgent'));
  });

  it('should declare static init method', () => {
    assert.ok(indexTs.includes('static init(): Promise<void>'));
  });

  it('should declare static fromConfig method', () => {
    assert.ok(indexTs.includes('static fromConfig(configPath: string): Promise<PylotAgent>'));
  });

  it('should declare chat method', () => {
    assert.ok(indexTs.includes('chat(message: string): Promise<string>'));
  });

  it('should declare static doctor method', () => {
    assert.ok(indexTs.includes('static doctor(): Promise<void>'));
  });

  it('should declare static status method', () => {
    assert.ok(indexTs.includes('static status(): Promise<void>'));
  });
});

// ── CLI script integrity ─────────────────────────────────────────────

describe('CLI script', () => {
  const cliJs = readFileSync(join(nodeDir, 'js', 'cli.js'), 'utf-8');

  it('should have shebang line', () => {
    assert.ok(cliJs.startsWith('#!/usr/bin/env node'));
  });

  it('should use execFileSync', () => {
    assert.ok(cliJs.includes('execFileSync'));
  });

  it('should delegate to the pylot binary', () => {
    assert.ok(cliJs.includes("'pylot'"));
  });

  it('should forward process arguments', () => {
    assert.ok(cliJs.includes('process.argv.slice(2)'));
  });

  it('should resolve the real binary rather than call itself by name', () => {
    // Guards against the self-recursion foot-gun: the wrapper must not spawn
    // the bare command that resolves back to this same shim.
    assert.ok(cliJs.includes('resolveBinary'));
    assert.ok(!cliJs.includes("execFileSync('pylot'"));
  });

  it('should handle a missing binary gracefully', () => {
    assert.ok(cliJs.includes('native `pylot` binary was not found'));
  });
});

// ── Cargo.toml consistency ───────────────────────────────────────────

describe('Node Cargo.toml consistency', () => {
  const cargoToml = readFileSync(join(nodeDir, 'Cargo.toml'), 'utf-8');

  it('should use the napi crate', () => {
    assert.ok(cargoToml.includes('napi'));
  });

  it('should use cdylib crate type', () => {
    assert.ok(cargoToml.includes('cdylib'));
  });

  it('should have a version matching package.json', () => {
    const pkg = JSON.parse(readFileSync(join(nodeDir, 'package.json'), 'utf-8'));
    assert.ok(cargoToml.includes(`version = "${pkg.version}"`));
  });

  it('should depend on napi and napi-derive', () => {
    assert.ok(cargoToml.includes('napi'));
    assert.ok(cargoToml.includes('napi-derive'));
  });
});

// ── build.rs exists and is valid ─────────────────────────────────────

describe('build.rs', () => {
  it('should exist', () => {
    assert.ok(existsSync(join(nodeDir, 'build.rs')));
  });

  it('should call napi_build::setup', () => {
    const buildRs = readFileSync(join(nodeDir, 'build.rs'), 'utf-8');
    assert.ok(buildRs.includes('napi_build::setup()'));
  });
});
