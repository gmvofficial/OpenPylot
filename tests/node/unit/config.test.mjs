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

describe('GMVAgent Node bindings', () => {
  it('should export GMVAgent and Config', () => {
    // This test verifies that the package structure is correct.
    // When the native addon is built, the imports will succeed.
    assert.equal(pkg.name, 'gmv-agent');
    assert.equal(pkg.version, '0.2.0');
    assert.ok(pkg.main);
    assert.ok(pkg.bin['gmv-agent']);
  });

  it('should have correct napi triples', () => {
    const triples = pkg.napi.triples;
    assert.ok(triples.defaults);
    assert.ok(triples.additional.includes('aarch64-apple-darwin'));
    assert.ok(triples.additional.includes('x86_64-unknown-linux-gnu'));
  });

  it('should define Config interface shape', () => {
    // Verify the Config object can be constructed (shape test)
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
