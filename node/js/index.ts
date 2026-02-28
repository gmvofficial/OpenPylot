/**
 * GMV Agent — Node.js / TypeScript bindings.
 *
 * @example
 * ```typescript
 * import { GMVAgent, Config } from 'gmv-agent';
 *
 * // Interactive setup
 * await GMVAgent.init();
 *
 * // Programmatic
 * const agent = await GMVAgent.fromConfig('~/.gmv-agent/secrets.enc');
 * const response = await agent.chat('What meetings do I have today?');
 * console.log(response);
 * ```
 *
 * @module
 */

export interface Config {
  llmProvider: string;
  llmModel: string;
  openaiApiKey?: string;
  anthropicApiKey?: string;
  googleCredentialsFile?: string;
  telegramBotToken?: string;
  telegramChatId?: string;
}

export declare class GMVAgent {
  /** Launch the interactive setup wizard. */
  static init(): Promise<void>;

  /** Initialize from an existing config/secrets file. */
  static fromConfig(configPath: string): Promise<GMVAgent>;

  /** Create a new agent from a programmatic Config object. */
  constructor(config: Config);

  /** Send a message and get a response. */
  chat(message: string): Promise<string>;

  /** Run diagnostic checks. */
  static doctor(): Promise<void>;

  /** Show status and connected services. */
  static status(): Promise<void>;
}

// Re-export from the native addon
let native;
try {
  native = require('../gmv_agent.node');
} catch {
  // Fallback: try platform-specific binary
  const os = require('os');
  const path = require('path');
  const platform = os.platform();
  const arch = os.arch();
  const tripleName = `gmv-agent.${platform}-${arch}.node`;
  native = require(path.join(__dirname, '..', tripleName));
}

module.exports = native;
module.exports.GMVAgent = native.GMVAgent;
module.exports.Config = native.Config;
