import { describe, it } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';

const __dirname = dirname(fileURLToPath(import.meta.url));
const nodeDir = join(__dirname, '..', '..', '..', 'node');
const pkg = JSON.parse(readFileSync(join(nodeDir, 'package.json'), 'utf-8'));

// ── Package metadata ─────────────────────────────────────────────────

describe('Package metadata', () => {
  it('should have correct name', () => {
    assert.equal(pkg.name, 'openpylot');
  });

  it('should have version 0.1.1', () => {
    assert.equal(pkg.version, '0.1.1');
  });

  it('should have a main entry point', () => {
    assert.equal(pkg.main, 'index.js');
  });

  it('should have TypeScript types', () => {
    assert.equal(pkg.types, 'index.d.ts');
  });

  it('should register the pylot CLI binary', () => {
    assert.ok(pkg.bin);
    assert.equal(pkg.bin['pylot'], 'js/cli.js');
  });

  it('should have Apache-2.0 license', () => {
    assert.equal(pkg.license, 'Apache-2.0');
  });

  it('should include the correct keywords', () => {
    assert.ok(pkg.keywords.includes('ai'));
    assert.ok(pkg.keywords.includes('agent'));
    assert.ok(pkg.keywords.includes('rust'));
    assert.ok(pkg.keywords.includes('napi'));
  });

  it('should publish with public access', () => {
    assert.equal(pkg.publishConfig?.access, 'public');
  });
});

// ── NAPI configuration ──────────────────────────────────────────────

describe('NAPI configuration', () => {
  it('should have napi config', () => {
    assert.ok(pkg.napi);
    assert.equal(pkg.napi.name, 'openpylot');
  });

  it('should pin explicit triples (defaults disabled)', () => {
    assert.equal(pkg.napi.triples.defaults, false);
  });

  it('should support x86_64-apple-darwin', () => {
    assert.ok(pkg.napi.triples.additional.includes('x86_64-apple-darwin'));
  });

  it('should support aarch64-apple-darwin', () => {
    assert.ok(pkg.napi.triples.additional.includes('aarch64-apple-darwin'));
  });

  it('should support x86_64-unknown-linux-gnu', () => {
    assert.ok(pkg.napi.triples.additional.includes('x86_64-unknown-linux-gnu'));
  });

  it('should support aarch64-unknown-linux-gnu', () => {
    assert.ok(pkg.napi.triples.additional.includes('aarch64-unknown-linux-gnu'));
  });

  it('should support x86_64-pc-windows-msvc', () => {
    assert.ok(pkg.napi.triples.additional.includes('x86_64-pc-windows-msvc'));
  });

  it('should declare a per-platform optional dependency for each triple', () => {
    const optional = pkg.optionalDependencies || {};
    for (const name of [
      'openpylot-darwin-x64',
      'openpylot-darwin-arm64',
      'openpylot-linux-x64-gnu',
      'openpylot-linux-arm64-gnu',
      'openpylot-win32-x64-msvc',
    ]) {
      assert.equal(optional[name], pkg.version, `${name} should match package version`);
    }
  });
});

// ── File structure validation ────────────────────────────────────────

describe('File structure', () => {
  it('should have the cli.js entry point', () => {
    const cliContent = readFileSync(join(nodeDir, 'js', 'cli.js'), 'utf-8');
    assert.ok(cliContent.startsWith('#!/usr/bin/env node'));
    assert.ok(cliContent.includes('pylot'));
  });

  it('should have generated index.d.ts type declarations', () => {
    const indexContent = readFileSync(join(nodeDir, 'index.d.ts'), 'utf-8');
    assert.ok(indexContent.includes('interface Config'));
    assert.ok(indexContent.includes('class PylotAgent'));
  });

  it('should whitelist the files that ship to npm', () => {
    assert.ok(pkg.files.includes('index.js'));
    assert.ok(pkg.files.includes('index.d.ts'));
    assert.ok(pkg.files.includes('js/cli.js'));
    assert.ok(pkg.files.includes('*.node'));
  });
});
