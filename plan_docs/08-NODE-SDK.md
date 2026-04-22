# 08 — Node.js SDK (NAPI-rs Bindings)

## Objective

Replace the current NAPI stubs with real bindings. Publish to npm as `openpylot`. Users should be able to `npm install openpylot` and use a native JavaScript/TypeScript API.

---

## Current State

- **Directory**: `node/`
- **Build**: NAPI-rs (`napi-build`, `napi-derive`)
- **Current implementation**: Stubs only, no real functionality
- **Files**: `node/src/lib.rs` (NAPI module), `node/js/` (JS wrapper)

---

## Architecture

### What to Expose

```typescript
import { Client, Pylot } from 'openpylot';

// 1. HTTP client (connects to server)
const client = new Client({ url: 'http://localhost:8000', apiKey: 'optional' });

// 2. Chat
const response = await client.chat('What\'s on my calendar?');
console.log(response.text);

// 3. Streaming
for await (const event of client.chatStream('Research AI safety')) {
  if (event.type === 'text_delta') process.stdout.write(event.text);
}

// 4. Memory
const results = await client.memory.search('preferences');
await client.memory.write('User prefers TypeScript', { type: 'preference' });

// 5. Tools
const tools = await client.tools.list();

// 6. Skills
const skills = await client.skills.list();
await client.skills.install('/path/to/SKILL.md');

// 7. Sub-agents
const agentId = await client.agents.spawn({
  name: 'researcher',
  prompt: 'Research X',
  tools: ['web_search'],
});
const result = await client.agents.waitFor(agentId, { timeout: 60000 });

// 8. Embedded mode (no server)
const pylot = new Pylot({ configPath: '~/.pylot/config.toml' });
const resp = await pylot.chat('Hello!');
```

### Module Structure

```
node/
├── Cargo.toml              # NAPI-rs config
├── package.json            # npm package
├── src/
│   └── lib.rs             # NAPI-rs module (Rust → JS bridge)
├── js/
│   ├── index.ts           # Package entry (TypeScript)
│   ├── client.ts          # HTTP client
│   ├── memory.ts          # Memory API wrapper
│   ├── tools.ts           # Tools API wrapper
│   ├── skills.ts          # Skills API wrapper
│   ├── agents.ts          # Sub-agent API wrapper
│   ├── streaming.ts       # SSE streaming client
│   └── types.ts           # TypeScript interfaces
├── index.d.ts             # Type declarations (generated)
└── __tests__/
    ├── client.test.ts
    └── memory.test.ts
```

---

## Implementation Steps

### Step 1: Update NAPI-rs bindings (Day 1)

**File**: `node/src/lib.rs`

```rust
use napi::bindgen_prelude::*;
use napi_derive::napi;

#[napi]
pub struct Pylot {
    inner: Arc<tokio::sync::Mutex<crate::Agent>>,
}

#[napi]
impl Pylot {
    #[napi(constructor)]
    pub fn new(config_path: Option<String>) -> Result<Self> {
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let agent = runtime.block_on(async {
            let config = crate::config::load_config(config_path.as_deref()).await?;
            crate::Agent::new(config).await
        }).map_err(|e| napi::Error::from_reason(e.to_string()))?;

        Ok(Self {
            inner: Arc::new(tokio::sync::Mutex::new(agent)),
        })
    }

    #[napi]
    pub async fn chat(&self, message: String) -> Result<String> {
        let agent = self.inner.lock().await;
        agent.handle_message(&message).await
            .map_err(|e| napi::Error::from_reason(e.to_string()))
    }

    #[napi]
    pub async fn memory_search(&self, query: String, limit: Option<u32>) -> Result<Vec<serde_json::Value>> {
        // delegate to memory store
    }
}
```

### Step 2: Implement TypeScript HTTP client (Day 1)

**File**: `node/js/client.ts`

```typescript
import { EventSource } from 'eventsource';

export interface ClientOptions {
  url?: string;
  apiKey?: string;
}

export class Client {
  private url: string;
  private headers: Record<string, string>;
  public memory: MemoryAPI;
  public tools: ToolsAPI;
  public skills: SkillsAPI;
  public agents: AgentsAPI;

  constructor(options: ClientOptions = {}) {
    this.url = (options.url || 'http://localhost:8000').replace(/\/$/, '');
    this.headers = { 'Content-Type': 'application/json' };
    if (options.apiKey) {
      this.headers['Authorization'] = `Bearer ${options.apiKey}`;
    }
    this.memory = new MemoryAPI(this);
    this.tools = new ToolsAPI(this);
    this.skills = new SkillsAPI(this);
    this.agents = new AgentsAPI(this);
  }

  async fetch(path: string, options: RequestInit = {}): Promise<Response> {
    const resp = await fetch(`${this.url}${path}`, {
      ...options,
      headers: { ...this.headers, ...options.headers },
    });
    if (!resp.ok) throw new Error(`HTTP ${resp.status}: ${await resp.text()}`);
    return resp;
  }

  async chat(message: string): Promise<ChatResponse> {
    const resp = await this.fetch('/api/chat', {
      method: 'POST',
      body: JSON.stringify({ message }),
    });
    return resp.json();
  }

  async *chatStream(message: string): AsyncGenerator<StreamEvent> {
    const resp = await this.fetch('/api/chat/stream', {
      method: 'POST',
      body: JSON.stringify({ message }),
    });

    const reader = resp.body!.getReader();
    const decoder = new TextDecoder();
    let buffer = '';

    while (true) {
      const { done, value } = await reader.read();
      if (done) break;
      buffer += decoder.decode(value, { stream: true });

      const lines = buffer.split('\n');
      buffer = lines.pop() || '';

      for (const line of lines) {
        if (line.startsWith('data: ')) {
          const event: StreamEvent = JSON.parse(line.slice(6));
          yield event;
          if (event.type === 'message_stop') return;
        }
      }
    }
  }
}
```

### Step 3: Types (Day 1)

**File**: `node/js/types.ts`

```typescript
export interface ChatResponse {
  text: string;
  toolCalls?: ToolCall[];
  usage?: { inputTokens: number; outputTokens: number };
}

export interface StreamEvent {
  type: 'text_delta' | 'tool_use_start' | 'tool_input_delta' | 'tool_result' | 'message_stop' | 'error';
  text?: string;
  toolName?: string;
  toolId?: string;
  result?: string;
  isError?: boolean;
}

export interface MemoryResult {
  id: string;
  content: string;
  memoryType: string;
  score?: number;
  entities?: string[];
  topics?: string[];
}

export interface ToolInfo { name: string; description: string; parameters?: object; }
export interface SkillInfo { name: string; description: string; category?: string; version?: string; }
export interface AgentState { id: string; name: string; status: string; result?: string; }
```

### Step 4: Sub-module APIs (Day 2)

Similar pattern for `memory.ts`, `tools.ts`, `skills.ts`, `agents.ts` — each wraps HTTP calls to the respective API endpoints.

### Step 5: Update package.json (Day 2)

```json
{
  "name": "openpylot",
  "version": "1.0.0",
  "description": "OpenPylot AI Assistant — Node.js SDK",
  "main": "dist/index.js",
  "types": "dist/index.d.ts",
  "scripts": {
    "build": "tsc && napi build --release",
    "test": "jest",
    "prepublishOnly": "npm run build"
  },
  "napi": {
    "name": "openpylot",
    "triples": {
      "defaults": true,
      "additional": ["aarch64-apple-darwin", "x86_64-unknown-linux-gnu"]
    }
  },
  "dependencies": {},
  "devDependencies": {
    "@napi-rs/cli": "^2",
    "typescript": "^5",
    "jest": "^29",
    "@types/jest": "^29",
    "ts-jest": "^29"
  }
}
```

### Step 6: Tests (Day 2)

```typescript
// node/__tests__/client.test.ts
import { Client } from '../js/client';

describe('Client', () => {
  const client = new Client({ url: 'http://localhost:8000' });

  test('chat returns response', async () => {
    const resp = await client.chat('Hello');
    expect(resp.text).toBeTruthy();
  });

  test('memory search returns array', async () => {
    const results = await client.memory.search('test');
    expect(Array.isArray(results)).toBe(true);
  });
});
```

---

## Acceptance Criteria

- [ ] `npm install openpylot` works
- [ ] HTTP client connects to server
- [ ] `client.chat()` returns response
- [ ] `client.chatStream()` yields events
- [ ] Memory, tools, skills, agents APIs work
- [ ] TypeScript types provided
- [ ] Embedded `Pylot` mode works via NAPI-rs
- [ ] Tests pass
