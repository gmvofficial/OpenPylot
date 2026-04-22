/**
 * OpenPylot — Node.js / TypeScript bindings.
 *
 * @example
 * ```typescript
 * import { PylotAgent, Config } from 'openpylot';
 *
 * // Interactive setup
 * await PylotAgent.init();
 *
 * // Programmatic
 * const agent = await PylotAgent.fromConfig('~/.pylot/secrets.enc');
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

export declare class PylotAgent {
  /** Launch the interactive setup wizard. */
  static init(): Promise<void>;

  /** Initialize from an existing config/secrets file. */
  static fromConfig(configPath: string): Promise<PylotAgent>;

  /** Create a new agent from a programmatic Config object. */
  constructor(config: Config);

  /** Send a message and get a response. */
  chat(message: string): Promise<string>;

  /** Run diagnostic checks. */
  static doctor(): Promise<void>;

  /** Show status and connected services. */
  static status(): Promise<void>;
}

export declare class PylotMemory {
  constructor(dbPath?: string);
  remember(content: string, memoryType?: string): string;
  search(query: string, limit?: number): Array<{ id: string; content: string; score: number }>;
  count(): number;
}

export declare class PylotSkills {
  constructor(skillsDir?: string);
  list(): Array<{ name: string; has_skill_file: boolean }>;
}

export declare class PylotLearning {
  constructor(dbPath?: string);
  rules(): Array<{ id: string; rule_text: string; confidence: number }>;
  feedback(sessionId: string, turnId: string, rating: number, comment?: string): void;
}

// Re-export from the native addon
let native;
try {
  native = require('../pylot.node');
} catch {
  // Fallback: try platform-specific binary
  const os = require('os');
  const path = require('path');
  const platform = os.platform();
  const arch = os.arch();
  const tripleName = `openpylot.${platform}-${arch}.node`;
  native = require(path.join(__dirname, '..', tripleName));
}

module.exports = native;
module.exports.PylotAgent = native.PylotAgent;
module.exports.Config = native.Config;
module.exports.PylotMemory = native.PylotMemory;
module.exports.PylotSkills = native.PylotSkills;
module.exports.PylotLearning = native.PylotLearning;
