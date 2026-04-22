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

  it('should have version 0.2.0', () => {
    assert.equal(pkg.version, '0.2.0');
  });

  it('should have a main entry point', () => {
    assert.ok(pkg.main);
    assert.equal(pkg.main, 'js/index.js');
  });

  it('should have TypeScript types', () => {
    assert.equal(pkg.types, 'js/index.d.ts');
  });

  it('should have the CLI binary registered', () => {
    assert.ok(pkg.bin);
    assert.ok(pkg.bin['openpylot']);
    assert.equal(pkg.bin['openpylot'], 'js/cli.js');
  });

  it('should have MIT license', () => {
    assert.equal(pkg.license, 'MIT');
  });

  it('should include the correct keywords', () => {
    assert.ok(pkg.keywords.includes('ai'));
    assert.ok(pkg.keywords.includes('agent'));
    assert.ok(pkg.keywords.includes('rust'));
    assert.ok(pkg.keywords.includes('napi'));
  });
});

// ── NAPI configuration ──────────────────────────────────────────────

describe('NAPI configuration', () => {
  it('should have napi config', () => {
    assert.ok(pkg.napi);
    assert.equal(pkg.napi.name, 'openpylot');
  });

  it('should have default triples enabled', () => {
    assert.ok(pkg.napi.triples.defaults);
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
});

// ── Config interface shape ───────────────────────────────────────────

describe('Config interface', () => {
  it('should have all expected fields', () => {
    const config = {
      llmProvider: 'openai',
      llmModel: 'gpt-4o',
      openaiApiKey: 'sk-test',
      anthropicApiKey: undefined,
      googleCredentialsFile: undefined,
      telegramBotToken: undefined,
      telegramChatId: undefined,
    };

    assert.equal(config.llmProvider, 'openai');
    assert.equal(config.llmModel, 'gpt-4o');
    assert.equal(config.openaiApiKey, 'sk-test');
    assert.equal(config.anthropicApiKey, undefined);
    assert.equal(config.googleCredentialsFile, undefined);
    assert.equal(config.telegramBotToken, undefined);
    assert.equal(config.telegramChatId, undefined);
  });

  it('should support anthropic configuration', () => {
    const config = {
      llmProvider: 'anthropic',
      llmModel: 'claude-sonnet-4-20250514',
      anthropicApiKey: 'sk-ant-test',
    };

    assert.equal(config.llmProvider, 'anthropic');
    assert.equal(config.llmModel, 'claude-sonnet-4-20250514');
    assert.equal(config.anthropicApiKey, 'sk-ant-test');
  });

  it('should allow optional fields to be omitted', () => {
    const config = {
      llmProvider: 'openai',
      llmModel: 'gpt-4o',
    };

    assert.equal(config.llmProvider, 'openai');
    assert.equal(config.openaiApiKey, undefined);
  });
});

// ── File structure validation ────────────────────────────────────────

describe('File structure', () => {
  it('should have cli.js entry point', () => {
    const cliContent = readFileSync(join(nodeDir, 'js', 'cli.js'), 'utf-8');
    assert.ok(cliContent.includes('#!/usr/bin/env node'));
    assert.ok(cliContent.includes('openpylot'));
  });

  it('should have index.ts with type declarations', () => {
    const indexContent = readFileSync(join(nodeDir, 'js', 'index.ts'), 'utf-8');
    assert.ok(indexContent.includes('interface Config'));
    assert.ok(indexContent.includes('class PylotAgent'));
  });

  it('should have package files pattern', () => {
    assert.ok(pkg.files.includes('js/**/*.js'));
    assert.ok(pkg.files.includes('js/**/*.d.ts'));
    assert.ok(pkg.files.includes('*.node'));
  });
});
