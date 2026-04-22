# 12 — Web UI & Documentation

## Objective

Build an embedded web dashboard for OpenPylot (gateway-style UI) and comprehensive API/developer documentation. The UI provides chat, memory browser, jobs dashboard, skills manager, social media dashboard, and settings — all served from the Axum API server.

---

## Current State

- **Frontend**: `frontend/` with Next.js, basic chat interface only
- **API**: REST endpoints in `src/api/` (chat, tools, memory, health, oauth)
- **Docs**: README.md, INSTALLATION.md — no API docs, no OpenAPI spec
- **Missing**: Dashboard UI, memory browser, jobs viewer, skills/agents manager, social dashboard, OpenAPI spec

---

## Reference Implementations

### IronClaw Gateway UI
- Embedded SPA served from Axum, built with Leptos (Rust WASM)
- Panels: Chat, Memory, Tools, Skills, Jobs, Settings, Extensions
- Real-time streaming via SSE
- Mobile-responsive

### Postiz App Frontend
- Next.js dashboard with full social media management
- Campaign calendar, analytics charts, media library
- Team collaboration, multi-org support
- 36+ platform integrations with visual status

---

## Architecture

```
frontend/
├── src/
│   ├── app/
│   │   ├── page.tsx              # Dashboard home
│   │   ├── layout.tsx            # App shell with sidebar
│   │   ├── chat/
│   │   │   └── page.tsx          # Chat interface with streaming
│   │   ├── memory/
│   │   │   └── page.tsx          # Memory browser
│   │   ├── skills/
│   │   │   └── page.tsx          # Skills manager
│   │   ├── agents/
│   │   │   └── page.tsx          # Sub-agents manager
│   │   ├── social/
│   │   │   ├── page.tsx          # Social dashboard
│   │   │   ├── campaigns/
│   │   │   │   └── page.tsx      # Campaign manager
│   │   │   └── analytics/
│   │   │       └── page.tsx      # Analytics
│   │   ├── jobs/
│   │   │   └── page.tsx          # Background jobs
│   │   ├── mcp/
│   │   │   └── page.tsx          # MCP server manager
│   │   ├── learning/
│   │   │   └── page.tsx          # Learning insights
│   │   └── settings/
│   │       └── page.tsx          # Configuration
│   ├── components/
│   │   ├── ChatPanel.tsx         # Streaming chat with tool calls
│   │   ├── MemoryTable.tsx       # Memory entries with search
│   │   ├── SkillCard.tsx         # Skill display card
│   │   ├── AgentCard.tsx         # Agent display card
│   │   ├── SocialPostEditor.tsx  # Post composer
│   │   ├── CampaignCalendar.tsx  # Visual campaign timeline
│   │   ├── AnalyticsChart.tsx    # Charts (recharts)
│   │   ├── JobsTable.tsx         # Background jobs list
│   │   ├── Sidebar.tsx           # Navigation sidebar
│   │   ├── Header.tsx            # Top bar with status
│   │   └── StreamingMessage.tsx  # SSE message renderer
│   ├── hooks/
│   │   ├── useChat.ts            # Chat with SSE streaming
│   │   ├── useMemory.ts          # Memory CRUD
│   │   ├── useSkills.ts          # Skills management
│   │   ├── useSocial.ts          # Social media operations
│   │   └── useWebSocket.ts       # WebSocket connection
│   └── lib/
│       ├── api.ts                # API client
│       └── types.ts              # TypeScript types
```

---

## Implementation Steps

### Step 1: OpenAPI specification (Day 1)

**File**: Create `docs/openapi.yaml`

Generate from Axum routes using `utoipa`:

```rust
// In Cargo.toml: utoipa = { version = "4", features = ["axum_extras"] }
//                utoipa-swagger-ui = { version = "4", features = ["axum"] }

use utoipa::OpenApi;

#[derive(OpenApi)]
#[openapi(
    paths(
        // Chat
        api::chat::send_message,
        api::chat::stream_message,
        // Memory
        api::memory::search,
        api::memory::store,
        api::memory::list_types,
        api::memory::consolidate,
        // Skills
        api::skills::list,
        api::skills::get,
        api::skills::install,
        api::skills::remove,
        // Agents
        api::agents::list,
        api::agents::spawn,
        api::agents::status,
        api::agents::terminate,
        // Social
        api::social::platforms,
        api::social::create_post,
        api::social::schedule_post,
        api::social::campaigns,
        api::social::analytics,
        // MCP
        api::mcp::list_servers,
        api::mcp::add_server,
        api::mcp::remove_server,
        api::mcp::list_tools,
        // Jobs
        api::jobs::list,
        api::jobs::cancel,
        api::jobs::status,
        // Learning
        api::learning::insights,
        api::learning::prompt_versions,
        api::learning::extracted_skills,
        // System
        api::system::health,
        api::system::status,
        api::system::config,
        api::system::doctor,
        api::system::version,
        // OAuth
        api::oauth::authorize,
        api::oauth::callback,
        api::oauth::status,
    ),
    components(schemas(
        ChatRequest, ChatResponse, StreamEvent,
        MemoryEntry, MemoryType, MemorySearch,
        Skill, SkillInstallRequest,
        SubAgent, SpawnAgentRequest,
        SocialPost, Campaign, PlatformAnalytics,
        McpServer, McpTool,
        Job, JobStatus,
        LearningInsight, PromptVersion,
        HealthStatus, SystemStatus, DoctorReport,
    ))
)]
pub struct ApiDoc;

// Serve Swagger UI
pub fn swagger_routes() -> Router {
    Router::new()
        .merge(SwaggerUi::new("/docs").url("/api-doc/openapi.json", ApiDoc::openapi()))
}
```

### Step 2: Dashboard layout & navigation (Day 2)

**File**: `frontend/src/app/layout.tsx`

```tsx
import { Sidebar } from '@/components/Sidebar';
import { Header } from '@/components/Header';

export default function RootLayout({ children }: { children: React.ReactNode }) {
  return (
    <html lang="en">
      <body className="bg-gray-950 text-gray-100">
        <div className="flex h-screen">
          <Sidebar />
          <div className="flex flex-1 flex-col overflow-hidden">
            <Header />
            <main className="flex-1 overflow-y-auto p-6">
              {children}
            </main>
          </div>
        </div>
      </body>
    </html>
  );
}
```

**File**: `frontend/src/components/Sidebar.tsx`

```tsx
const navItems = [
  { href: '/chat', icon: MessageSquare, label: 'Chat' },
  { href: '/memory', icon: Brain, label: 'Memory' },
  { href: '/skills', icon: Puzzle, label: 'Skills' },
  { href: '/agents', icon: Users, label: 'Agents' },
  { href: '/social', icon: Share2, label: 'Social' },
  { href: '/jobs', icon: Clock, label: 'Jobs' },
  { href: '/mcp', icon: Plug, label: 'MCP' },
  { href: '/learning', icon: TrendingUp, label: 'Learning' },
  { href: '/settings', icon: Settings, label: 'Settings' },
];
```

### Step 3: Streaming chat panel (Day 3)

**File**: `frontend/src/hooks/useChat.ts`

```tsx
export function useChat() {
  const [messages, setMessages] = useState<Message[]>([]);
  const [isStreaming, setIsStreaming] = useState(false);

  const sendMessage = async (content: string) => {
    setMessages(prev => [...prev, { role: 'user', content }]);
    setIsStreaming(true);

    const response = await fetch('/api/chat/stream', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ message: content }),
    });

    const reader = response.body!.getReader();
    const decoder = new TextDecoder();
    let assistantMessage = '';

    while (true) {
      const { done, value } = await reader.read();
      if (done) break;

      const chunk = decoder.decode(value);
      const lines = chunk.split('\n').filter(l => l.startsWith('data: '));

      for (const line of lines) {
        const event: StreamEvent = JSON.parse(line.slice(6));
        switch (event.type) {
          case 'text_delta':
            assistantMessage += event.content;
            setMessages(prev => {
              const updated = [...prev];
              const last = updated[updated.length - 1];
              if (last?.role === 'assistant') {
                last.content = assistantMessage;
              } else {
                updated.push({ role: 'assistant', content: assistantMessage });
              }
              return updated;
            });
            break;
          case 'tool_use':
            // Show tool call inline
            break;
          case 'done':
            setIsStreaming(false);
            break;
        }
      }
    }
  };

  return { messages, isStreaming, sendMessage };
}
```

### Step 4: Memory browser (Day 4)

**File**: `frontend/src/app/memory/page.tsx`

```tsx
export default function MemoryPage() {
  const [query, setQuery] = useState('');
  const [memoryType, setMemoryType] = useState<string>('all');
  const [entries, setEntries] = useState<MemoryEntry[]>([]);

  return (
    <div className="space-y-6">
      <h1 className="text-2xl font-bold">Memory Browser</h1>

      {/* Search bar */}
      <div className="flex gap-4">
        <input
          value={query}
          onChange={e => setQuery(e.target.value)}
          placeholder="Search memory..."
          className="flex-1 rounded-lg bg-gray-800 px-4 py-2"
        />
        <select value={memoryType} onChange={e => setMemoryType(e.target.value)}
                className="rounded-lg bg-gray-800 px-4 py-2">
          <option value="all">All Types</option>
          <option value="episodic">Episodic</option>
          <option value="semantic">Semantic</option>
          <option value="preference">Preference</option>
          <option value="procedural">Procedural</option>
          <option value="project">Project State</option>
          <option value="working">Working Summary</option>
        </select>
      </div>

      {/* Memory table */}
      <MemoryTable entries={entries} />

      {/* Memory stats */}
      <MemoryStats />
    </div>
  );
}
```

### Step 5: Social media dashboard (Day 5)

**File**: `frontend/src/app/social/page.tsx`

```tsx
export default function SocialPage() {
  return (
    <div className="space-y-6">
      <h1 className="text-2xl font-bold">Social Media</h1>

      {/* Connected platforms */}
      <PlatformGrid />

      {/* Quick compose */}
      <SocialPostEditor />

      {/* Upcoming scheduled posts */}
      <ScheduledPosts />

      {/* Recent analytics */}
      <AnalyticsOverview />
    </div>
  );
}
```

Key components:
- **PlatformGrid**: Shows connected platforms with status indicators
- **SocialPostEditor**: Rich text editor with AI content generation, platform preview, scheduling
- **CampaignCalendar**: `react-big-calendar` or `@fullcalendar/react` for visual timeline
- **AnalyticsChart**: `recharts` for engagement metrics per platform

### Step 6: Skills & agents manager (Day 5)

**File**: `frontend/src/app/skills/page.tsx`

```tsx
export default function SkillsPage() {
  const { skills, install, remove } = useSkills();

  return (
    <div className="space-y-6">
      <div className="flex items-center justify-between">
        <h1 className="text-2xl font-bold">Skills</h1>
        <button className="btn-primary">Install Skill</button>
      </div>

      {/* Active skills */}
      <div className="grid grid-cols-1 gap-4 md:grid-cols-2 lg:grid-cols-3">
        {skills.map(skill => (
          <SkillCard key={skill.name} skill={skill} onRemove={remove} />
        ))}
      </div>
    </div>
  );
}
```

### Step 7: Jobs dashboard (Day 6)

**File**: `frontend/src/app/jobs/page.tsx`

```tsx
export default function JobsPage() {
  return (
    <div className="space-y-6">
      <h1 className="text-2xl font-bold">Background Jobs</h1>
      <JobsTable />  {/* columns: Name, Type, Status, Started, Duration, Actions */}
    </div>
  );
}
```

### Step 8: Learning insights page (Day 6)

**File**: `frontend/src/app/learning/page.tsx`

```tsx
export default function LearningPage() {
  return (
    <div className="space-y-6">
      <h1 className="text-2xl font-bold">Learning & Insights</h1>

      {/* Prompt evolution timeline */}
      <PromptVersionHistory />

      {/* Extracted skills */}
      <ExtractedSkillsList />

      {/* RL metrics (if enabled) */}
      <RLMetrics />  {/* reward trend, win rate, policy generations */}

      {/* Error patterns */}
      <ErrorPatterns />
    </div>
  );
}
```

### Step 9: Settings page (Day 7)

**File**: `frontend/src/app/settings/page.tsx`

Tabs: General, LLM, Memory, Security, Social, MCP, Advanced

```tsx
export default function SettingsPage() {
  return (
    <div className="space-y-6">
      <h1 className="text-2xl font-bold">Settings</h1>
      <Tabs defaultValue="general">
        <TabsList>
          <Tab value="general">General</Tab>
          <Tab value="llm">LLM</Tab>
          <Tab value="memory">Memory</Tab>
          <Tab value="security">Security</Tab>
          <Tab value="social">Social</Tab>
          <Tab value="mcp">MCP</Tab>
          <Tab value="advanced">Advanced</Tab>
        </TabsList>
        {/* Tab content for each */}
      </Tabs>
    </div>
  );
}
```

### Step 10: Embed frontend in Axum (Day 7)

**File**: Modify `src/api/mod.rs`

```rust
use rust_embed::RustEmbed;

#[derive(RustEmbed)]
#[folder = "frontend/out"]  // Next.js static export
struct FrontendAssets;

pub fn create_router(state: AppState) -> Router {
    Router::new()
        // API routes
        .nest("/api", api_routes(state.clone()))
        // Swagger UI
        .merge(swagger_routes())
        // Frontend (catch-all for SPA)
        .fallback(serve_frontend)
        .with_state(state)
}

async fn serve_frontend(uri: axum::http::Uri) -> impl IntoResponse {
    let path = uri.path().trim_start_matches('/');
    // Try exact file, then index.html for SPA routing
    let file = FrontendAssets::get(path)
        .or_else(|| FrontendAssets::get("index.html"));

    match file {
        Some(content) => {
            let mime = mime_guess::from_path(path).first_or_octet_stream();
            (StatusCode::OK, [(header::CONTENT_TYPE, mime.as_ref())], content.data.into())
        }
        None => (StatusCode::NOT_FOUND, [(header::CONTENT_TYPE, "text/plain")], "Not Found".into()),
    }
}
```

Build command:
```bash
cd frontend && npm run build && npx next export
cargo build --release  # Embeds frontend/out/ via rust_embed
```

---

## Documentation Files

### Developer docs structure

```
docs/
├── INSTALLATION.md          # Updated install guide
├── QUICKSTART.md            # 5-minute getting started
├── API.md                   # REST API reference (from OpenAPI)
├── SKILLS.md                # Writing custom skills
├── AGENTS.md                # Sub-agent system guide
├── MEMORY.md                # Memory system overview
├── SOCIAL.md                # Social media setup
├── MCP.md                   # MCP integration guide
├── LEARNING.md              # Learning system overview
├── PYTHON-SDK.md            # Python SDK usage
├── NODE-SDK.md              # Node.js SDK usage
├── SECURITY.md              # Security model
├── ARCHITECTURE.md          # System architecture
└── CONTRIBUTING.md          # How to contribute
```

---

## Config Additions

```toml
[ui]
enabled = true
port = 3000         # embedded UI port (same as API)
theme = "dark"      # dark | light
```

---

## npm Dependencies

Add to `frontend/package.json`:
```json
{
  "dependencies": {
    "recharts": "^2.8",
    "@fullcalendar/react": "^6.1",
    "lucide-react": "^0.300",
    "eventsource-parser": "^1.0"
  }
}
```

---

## Testing

- `test_frontend_embed` — Frontend assets served correctly
- `test_swagger_ui` — OpenAPI spec loads at `/docs`
- `test_spa_routing` — All frontend routes return index.html
- `test_api_cors` — CORS headers set for dev mode
- Cypress/Playwright E2E tests for each dashboard page

---

## Acceptance Criteria

- [ ] `pylot serve` starts API server with embedded frontend
- [ ] Dashboard accessible at `http://localhost:3000`
- [ ] Chat page with SSE streaming works
- [ ] Memory browser: search, filter by type, view entries
- [ ] Skills page: list, install, remove
- [ ] Agents page: list, spawn, terminate
- [ ] Social dashboard: compose, schedule, view analytics
- [ ] Jobs page: list running and completed jobs
- [ ] Learning page: prompt versions, extracted skills, RL metrics
- [ ] Settings page: all categories editable
- [ ] Swagger UI at `/docs` with full OpenAPI spec
- [ ] All docs written in `docs/` directory
- [ ] Frontend builds and embeds in release binary via `rust_embed`
