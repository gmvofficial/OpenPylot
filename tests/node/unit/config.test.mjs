import { describe, it } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';

const __dirname = dirname(fileURLToPath(import.meta.url));
const nodeDir = join(__dirname, '..', '..', '..', 'node');
const pkg = JSON.parse(readFileSync(join(nodeDir, 'package.json'), 'utf-8'));

// NOTE: These tests validate the JavaScript wrapper layer.
// The native addon must be built first with `npm run build`.

describe('PylotAgent Node bindings', () => {
  it('should have the expected package identity', () => {
    assert.equal(pkg.name, 'openpylot');
    assert.equal(pkg.version, '0.1.1');
    assert.equal(pkg.main, 'index.js');
    assert.equal(pkg.bin['pylot'], 'js/cli.js');
  });

  it('should have the expected napi triples', () => {
    const triples = pkg.napi.triples;
    assert.equal(triples.defaults, false);
    assert.ok(triples.additional.includes('aarch64-apple-darwin'));
    assert.ok(triples.additional.includes('x86_64-unknown-linux-gnu'));
  });

  it('should define the Config interface shape', () => {
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
  });
});
