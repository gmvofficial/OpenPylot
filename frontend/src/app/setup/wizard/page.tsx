"use client";

import { useState, useEffect, useCallback } from "react";
import { useRouter } from "next/navigation";
import {
  Bot,
  Brain,
  ChevronRight,
  ChevronLeft,
  Check,
  Send,
  Calendar,
  Mail,
  MessageCircle,
  Loader2,
  Eye,
  EyeOff,
  Sparkles,
  Shield,
  Zap,
  ExternalLink,
} from "lucide-react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Textarea } from "@/components/ui/textarea";
import { Card, CardContent } from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";
import { cn } from "@/lib/utils";
import { useToastStore } from "@/stores/toast";

const STEPS = [
  { id: "welcome", label: "Welcome" },
  { id: "llm", label: "LLM Provider" },
  { id: "identity", label: "Agent Identity" },
  { id: "integrations", label: "Integrations" },
  { id: "complete", label: "Complete" },
];

const LLM_PROVIDERS = [
  {
    id: "anthropic",
    name: "Anthropic",
    description: "Claude Sonnet 4, Opus 4 — Best for reasoning & safety",
    models: ["claude-sonnet-4-20250514", "claude-opus-4-20250514"],
    envKey: "ANTHROPIC_API_KEY",
    placeholder: "sk-ant-...",
    icon: "🧠",
  },
  {
    id: "openai",
    name: "OpenAI",
    description: "GPT-4o, GPT-4.1 — Strong general-purpose models",
    models: ["gpt-4o", "gpt-4o-mini", "gpt-4.1"],
    envKey: "OPENAI_API_KEY",
    placeholder: "sk-proj-...",
    icon: "💡",
  },
  {
    id: "ollama",
    name: "Ollama (Local)",
    description: "Run models locally — Free, private, no API key",
    models: ["llama3.1", "mistral", "codellama"],
    envKey: "",
    placeholder: "",
    icon: "🏠",
  },
];

const PERSONAS = [
  { id: "professional", label: "Professional & concise", emoji: "💼" },
  { id: "friendly", label: "Friendly & conversational", emoji: "😊" },
  { id: "technical", label: "Technical & detailed", emoji: "🔧" },
  { id: "custom", label: "Custom persona", emoji: "✨" },
];

interface SetupState {
  // LLM
  provider: string;
  model: string;
  apiKey: string;
  // Identity
  agentName: string;
  userName: string;
  persona: string;
  customPersona: string;
  // Integrations
  telegramToken: string;
  telegramChatId: string;
  googleEnabled: boolean;
  telegramEnabled: boolean;
}

export default function SetupWizardPage() {
  const router = useRouter();
  const { addToast } = useToastStore();
  const [step, setStep] = useState(0);
  const [saving, setSaving] = useState(false);
  const [showApiKey, setShowApiKey] = useState(false);
  const [validating, setValidating] = useState(false);
  const [validationResult, setValidationResult] = useState<string | null>(null);
  const [setupStatus, setSetupStatus] = useState<Record<string, boolean>>({});

  const [state, setState] = useState<SetupState>({
    provider: "",
    model: "",
    apiKey: "",
    agentName: "Jarvis",
    userName: "",
    persona: "professional",
    customPersona: "",
    telegramToken: "",
    telegramChatId: "",
    googleEnabled: false,
    telegramEnabled: false,
  });

  const update = (field: keyof SetupState, value: string | boolean) => {
    setState((s) => ({ ...s, [field]: value }));
  };

  // Load existing setup status
  useEffect(() => {
    (async () => {
      try {
        const res = await fetch(`${window.location.origin}/api/setup/status`);
        if (res.ok) {
          const json = await res.json();
          const data = json.data ?? json;
          setSetupStatus(data);

          // Pre-fill from existing settings
          if (data.llm_configured) {
            try {
              const settingsRes = await fetch(`${window.location.origin}/api/settings`);
              if (settingsRes.ok) {
                const sJson = await settingsRes.json();
                const settings = sJson.data ?? sJson;
                if (settings.model) update("model", settings.model);
                if (settings.agent_name) update("agentName", settings.agent_name);
              }
            } catch {
              // ignore
            }
          }
        }
      } catch {
        // Backend not available
      }
    })();
  }, []);

  const validateApiKey = useCallback(async () => {
    if (!state.apiKey && state.provider !== "ollama") return;
    setValidating(true);
    setValidationResult(null);

    try {
      const res = await fetch(`${window.location.origin}/api/setup/validate-key`, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          provider: state.provider,
          api_key: state.apiKey,
        }),
      });
      const json = await res.json();
      const data = json.data ?? json;
      if (data.valid) {
        setValidationResult("valid");
      } else {
        setValidationResult(data.error || "Invalid API key");
      }
    } catch {
      setValidationResult("Could not validate — will save anyway");
    } finally {
      setValidating(false);
    }
  }, [state.apiKey, state.provider]);

  const saveLlmConfig = async () => {
    setSaving(true);
    try {
      const res = await fetch(`${window.location.origin}/api/setup/llm`, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          provider: state.provider,
          model: state.model,
          api_key: state.apiKey,
        }),
      });
      if (res.ok) {
        addToast({ title: "LLM configured", variant: "success" });
        return true;
      }
    } catch {
      addToast({ title: "Failed to save LLM config", variant: "error" });
    } finally {
      setSaving(false);
    }
    return false;
  };

  const saveIdentity = async () => {
    setSaving(true);
    try {
      const personaText =
        state.persona === "custom"
          ? state.customPersona
          : PERSONAS.find((p) => p.id === state.persona)?.label || state.persona;

      const res = await fetch(`${window.location.origin}/api/setup/identity`, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          agent_name: state.agentName,
          user_name: state.userName,
          persona: personaText,
        }),
      });
      if (res.ok) {
        addToast({ title: "Agent identity saved", variant: "success" });
        return true;
      }
    } catch {
      addToast({ title: "Failed to save identity", variant: "error" });
    } finally {
      setSaving(false);
    }
    return false;
  };

  const saveTelegram = async () => {
    if (!state.telegramToken) return true;
    setSaving(true);
    try {
      const res = await fetch(`${window.location.origin}/api/setup/telegram`, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          bot_token: state.telegramToken,
          chat_id: state.telegramChatId,
        }),
      });
      if (res.ok) {
        addToast({ title: "Telegram connected", variant: "success" });
        return true;
      }
    } catch {
      addToast({ title: "Failed to save Telegram config", variant: "error" });
    } finally {
      setSaving(false);
    }
    return false;
  };

  const handleNext = async () => {
    const currentStepId = STEPS[step].id;

    if (currentStepId === "llm") {
      if (!state.provider) {
        addToast({ title: "Please select an LLM provider", variant: "warning" });
        return;
      }
      if (state.provider !== "ollama" && !state.apiKey) {
        addToast({ title: "Please enter your API key", variant: "warning" });
        return;
      }
      if (!state.model) {
        addToast({ title: "Please select a model", variant: "warning" });
        return;
      }
      const ok = await saveLlmConfig();
      if (!ok) return;
    }

    if (currentStepId === "identity") {
      if (!state.agentName) {
        addToast({ title: "Please name your agent", variant: "warning" });
        return;
      }
      const ok = await saveIdentity();
      if (!ok) return;
    }

    if (currentStepId === "integrations") {
      await saveTelegram();
    }

    setStep((s) => Math.min(s + 1, STEPS.length - 1));
  };

  const handlePrev = () => {
    setStep((s) => Math.max(s - 1, 0));
  };

  const handleFinish = () => {
    router.replace("/chat");
  };

  const currentStep = STEPS[step];
  const progress = ((step + 1) / STEPS.length) * 100;

  return (
    <div className="min-h-full flex flex-col items-center justify-start py-8 px-4">
      {/* Progress bar */}
      <div className="w-full max-w-2xl mb-8">
        <div className="flex items-center justify-between mb-3">
          {STEPS.map((s, i) => (
            <div key={s.id} className="flex items-center gap-1.5">
              <div
                className={cn(
                  "w-8 h-8 rounded-full flex items-center justify-center text-xs font-medium transition-colors",
                  i < step
                    ? "bg-accent-success text-foreground"
                    : i === step
                      ? "bg-accent text-foreground"
                      : "bg-background-tertiary text-foreground-muted"
                )}
              >
                {i < step ? <Check className="w-4 h-4" /> : i + 1}
              </div>
              <span
                className={cn(
                  "text-xs hidden sm:inline",
                  i === step ? "text-foreground font-medium" : "text-foreground-muted"
                )}
              >
                {s.label}
              </span>
              {i < STEPS.length - 1 && (
                <div
                  className={cn(
                    "w-8 sm:w-12 h-0.5 mx-1",
                    i < step ? "bg-accent-success" : "bg-background-tertiary"
                  )}
                />
              )}
            </div>
          ))}
        </div>
        <div className="h-1 bg-background-tertiary rounded-full overflow-hidden">
          <div
            className="h-full bg-accent rounded-full transition-all duration-500"
            style={{ width: `${progress}%` }}
          />
        </div>
      </div>

      {/* Step Content */}
      <div className="w-full max-w-2xl">
        {/* Welcome */}
        {currentStep.id === "welcome" && (
          <div className="text-center py-8">
            <div className="flex items-center justify-center w-20 h-20 rounded-2xl bg-accent/10 mx-auto mb-6">
              <Bot className="w-10 h-10 text-accent" />
            </div>
            <h1 className="text-3xl font-bold text-foreground mb-3">
              Welcome to OpenPylot
            </h1>
            <p className="text-foreground-secondary text-lg mb-8 max-w-md mx-auto">
              Let&apos;s set up your personal AI assistant in a few quick steps.
              You can always change these settings later.
            </p>

            <div className="grid grid-cols-1 sm:grid-cols-3 gap-4 max-w-lg mx-auto mb-8">
              <div className="flex flex-col items-center gap-2 p-4 rounded-xl bg-background-secondary">
                <Sparkles className="w-6 h-6 text-accent" />
                <span className="text-sm text-foreground-secondary">AI-Powered</span>
              </div>
              <div className="flex flex-col items-center gap-2 p-4 rounded-xl bg-background-secondary">
                <Shield className="w-6 h-6 text-accent-success" />
                <span className="text-sm text-foreground-secondary">Privacy-First</span>
              </div>
              <div className="flex flex-col items-center gap-2 p-4 rounded-xl bg-background-secondary">
                <Zap className="w-6 h-6 text-accent-warning" />
                <span className="text-sm text-foreground-secondary">Rust-Powered</span>
              </div>
            </div>

            <Button size="lg" onClick={handleNext} className="px-8">
              Get Started
              <ChevronRight className="w-4 h-4 ml-1" />
            </Button>
          </div>
        )}

        {/* LLM Provider */}
        {currentStep.id === "llm" && (
          <div className="space-y-6">
            <div>
              <h2 className="text-2xl font-bold text-foreground mb-2">
                Choose Your LLM Provider
              </h2>
              <p className="text-foreground-secondary">
                Select which AI model will power your agent. You can change this anytime.
              </p>
            </div>

            <div className="grid gap-3">
              {LLM_PROVIDERS.map((p) => (
                <Card
                  key={p.id}
                  className={cn(
                    "cursor-pointer transition-all",
                    state.provider === p.id
                      ? "border-accent bg-accent/5"
                      : "hover:border-border-hover"
                  )}
                  onClick={() => {
                    update("provider", p.id);
                    update("model", p.models[0]);
                    setValidationResult(null);
                  }}
                >
                  <CardContent className="flex items-center gap-4 py-4">
                    <span className="text-2xl">{p.icon}</span>
                    <div className="flex-1">
                      <p className="font-medium text-foreground">{p.name}</p>
                      <p className="text-sm text-foreground-muted">{p.description}</p>
                    </div>
                    <div
                      className={cn(
                        "w-5 h-5 rounded-full border-2 flex items-center justify-center",
                        state.provider === p.id
                          ? "border-accent bg-accent"
                          : "border-foreground-muted"
                      )}
                    >
                      {state.provider === p.id && (
                        <Check className="w-3 h-3 text-foreground" />
                      )}
                    </div>
                  </CardContent>
                </Card>
              ))}
            </div>

            {state.provider && state.provider !== "ollama" && (
              <div className="space-y-4">
                {/* API Key */}
                <div className="space-y-2">
                  <label className="text-sm font-medium text-foreground">
                    API Key
                  </label>
                  <div className="relative">
                    <Input
                      type={showApiKey ? "text" : "password"}
                      value={state.apiKey}
                      onChange={(e) => {
                        update("apiKey", e.target.value);
                        setValidationResult(null);
                      }}
                      placeholder={
                        LLM_PROVIDERS.find((p) => p.id === state.provider)
                          ?.placeholder ?? "Enter API key"
                      }
                      className="pr-20"
                    />
                    <div className="absolute right-2 top-1/2 -translate-y-1/2 flex items-center gap-1">
                      <button
                        onClick={() => setShowApiKey(!showApiKey)}
                        className="p-1 text-foreground-muted hover:text-foreground"
                      >
                        {showApiKey ? (
                          <EyeOff className="w-4 h-4" />
                        ) : (
                          <Eye className="w-4 h-4" />
                        )}
                      </button>
                      <Button
                        size="sm"
                        variant="ghost"
                        onClick={validateApiKey}
                        disabled={!state.apiKey || validating}
                        className="h-7 text-xs"
                      >
                        {validating ? (
                          <Loader2 className="w-3 h-3 animate-spin" />
                        ) : (
                          "Verify"
                        )}
                      </Button>
                    </div>
                  </div>
                  {validationResult === "valid" && (
                    <p className="text-sm text-accent-success flex items-center gap-1">
                      <Check className="w-3.5 h-3.5" /> API key is valid
                    </p>
                  )}
                  {validationResult && validationResult !== "valid" && (
                    <p className="text-sm text-accent-warning">
                      {validationResult}
                    </p>
                  )}
                </div>

                {/* Model selector */}
                <div className="space-y-2">
                  <label className="text-sm font-medium text-foreground">
                    Default Model
                  </label>
                  <div className="flex flex-wrap gap-2">
                    {LLM_PROVIDERS.find((p) => p.id === state.provider)?.models.map(
                      (m, i) => (
                        <button
                          key={m}
                          onClick={() => update("model", m)}
                          className={cn(
                            "px-3 py-1.5 rounded-lg text-sm border transition-colors",
                            state.model === m
                              ? "border-accent bg-accent/10 text-accent font-medium"
                              : "border-border text-foreground-secondary hover:border-border-hover"
                          )}
                        >
                          {m}
                          {i === 0 && (
                            <span className="ml-1.5 text-[10px] text-accent-success">
                              recommended
                            </span>
                          )}
                        </button>
                      )
                    )}
                  </div>
                </div>
              </div>
            )}

            {state.provider === "ollama" && (
              <Card>
                <CardContent className="py-4">
                  <p className="text-sm text-foreground-secondary mb-3">
                    Make sure Ollama is installed and running locally.
                  </p>
                  <div className="space-y-2">
                    <label className="text-sm font-medium text-foreground">
                      Model
                    </label>
                    <div className="flex flex-wrap gap-2">
                      {LLM_PROVIDERS.find((p) => p.id === "ollama")?.models.map(
                        (m) => (
                          <button
                            key={m}
                            onClick={() => update("model", m)}
                            className={cn(
                              "px-3 py-1.5 rounded-lg text-sm border transition-colors",
                              state.model === m
                                ? "border-accent bg-accent/10 text-accent font-medium"
                                : "border-border text-foreground-secondary hover:border-border-hover"
                            )}
                          >
                            {m}
                          </button>
                        )
                      )}
                    </div>
                  </div>
                </CardContent>
              </Card>
            )}
          </div>
        )}

        {/* Agent Identity */}
        {currentStep.id === "identity" && (
          <div className="space-y-6">
            <div>
              <h2 className="text-2xl font-bold text-foreground mb-2">
                Name Your Agent
              </h2>
              <p className="text-foreground-secondary">
                Give your assistant a name and personality.
              </p>
            </div>

            <div className="space-y-4">
              <div className="space-y-2">
                <label className="text-sm font-medium text-foreground">
                  Agent Name
                </label>
                <Input
                  value={state.agentName}
                  onChange={(e) => update("agentName", e.target.value)}
                  placeholder="Jarvis"
                />
              </div>

              <div className="space-y-2">
                <label className="text-sm font-medium text-foreground">
                  Your Name
                </label>
                <Input
                  value={state.userName}
                  onChange={(e) => update("userName", e.target.value)}
                  placeholder="What should the agent call you?"
                />
              </div>

              <div className="space-y-2">
                <label className="text-sm font-medium text-foreground">
                  Persona Style
                </label>
                <div className="grid grid-cols-2 gap-3">
                  {PERSONAS.map((p) => (
                    <button
                      key={p.id}
                      onClick={() => update("persona", p.id)}
                      className={cn(
                        "flex items-center gap-3 p-3 rounded-xl border text-left transition-colors",
                        state.persona === p.id
                          ? "border-accent bg-accent/5"
                          : "border-border hover:border-border-hover"
                      )}
                    >
                      <span className="text-xl">{p.emoji}</span>
                      <span className="text-sm text-foreground">{p.label}</span>
                    </button>
                  ))}
                </div>
              </div>

              {state.persona === "custom" && (
                <div className="space-y-2">
                  <label className="text-sm font-medium text-foreground">
                    Custom Persona Description
                  </label>
                  <Textarea
                    value={state.customPersona}
                    onChange={(e) => update("customPersona", e.target.value)}
                    placeholder="Describe how you want your agent to behave..."
                    rows={4}
                  />
                </div>
              )}
            </div>
          </div>
        )}

        {/* Integrations */}
        {currentStep.id === "integrations" && (
          <div className="space-y-6">
            <div>
              <h2 className="text-2xl font-bold text-foreground mb-2">
                Connect Your Services
              </h2>
              <p className="text-foreground-secondary">
                Set up integrations to unlock your agent&apos;s full power.
                You can skip this and add them later.
              </p>
            </div>

            {/* Google */}
            <Card>
              <CardContent className="py-4">
                <div className="flex items-center gap-4">
                  <div className="flex items-center justify-center w-12 h-12 rounded-xl bg-accent/10">
                    <Calendar className="w-6 h-6 text-accent" />
                  </div>
                  <div className="flex-1">
                    <h3 className="font-medium text-foreground">
                      Google Calendar &amp; Gmail
                    </h3>
                    <p className="text-sm text-foreground-muted">
                      Manage events, send emails, and get meeting reminders.
                    </p>
                  </div>
                  {setupStatus.google_configured ? (
                    <Badge variant="success">Connected</Badge>
                  ) : (
                    <Button
                      size="sm"
                      variant="outline"
                      onClick={async () => {
                        try {
                          const res = await fetch(
                            `${window.location.origin}/api/integrations/google_calendar/connect`,
                            { method: "POST" }
                          );
                          const json = await res.json();
                          const data = json.data ?? json;
                          if (data.auth_url) {
                            window.open(data.auth_url, "_blank");
                          } else {
                            addToast({
                              title: "Google OAuth",
                              description:
                                data.message ||
                                "Run 'gmv-agent init --only google-calendar' from terminal to complete Google setup.",
                              variant: "info",
                            });
                          }
                        } catch {
                          addToast({
                            title: "Failed to initiate Google connection",
                            variant: "error",
                          });
                        }
                      }}
                    >
                      Connect
                    </Button>
                  )}
                </div>
              </CardContent>
            </Card>

            {/* Telegram */}
            <Card className={state.telegramToken ? "border-accent/30" : ""}>
              <CardContent className="py-4 space-y-4">
                <div className="flex items-center gap-4">
                  <div className="flex items-center justify-center w-12 h-12 rounded-xl bg-sky-500/10">
                    <Send className="w-6 h-6 text-sky-400" />
                  </div>
                  <div className="flex-1">
                    <h3 className="font-medium text-foreground">Telegram Bot</h3>
                    <p className="text-sm text-foreground-muted">
                      Chat with your agent on Telegram and receive notifications.
                    </p>
                  </div>
                  {setupStatus.telegram_configured ? (
                    <Badge variant="success">Connected</Badge>
                  ) : (
                    <Badge variant="outline">Manual Setup</Badge>
                  )}
                </div>

                {!setupStatus.telegram_configured && (
                  <div className="ml-16 space-y-3">
                    <div className="text-sm text-foreground-secondary space-y-1">
                      <p>1. Open Telegram and search for <strong>@BotFather</strong></p>
                      <p>2. Send <code className="bg-background-tertiary px-1.5 py-0.5 rounded text-accent-info">/newbot</code> and follow the prompts</p>
                      <p>3. Copy the API token below</p>
                    </div>
                    <a
                      href="https://t.me/BotFather"
                      target="_blank"
                      rel="noopener noreferrer"
                      className="inline-flex items-center gap-1.5 text-sm text-accent hover:underline"
                    >
                      <ExternalLink className="w-3.5 h-3.5" />
                      Open BotFather in Telegram
                    </a>

                    <div className="space-y-2">
                      <label className="text-sm font-medium text-foreground">
                        Bot Token
                      </label>
                      <Input
                        value={state.telegramToken}
                        onChange={(e) => update("telegramToken", e.target.value)}
                        placeholder="1234567890:ABCdefGhIjKlMnoPqRsTuVwXyZ"
                        type="password"
                      />
                    </div>

                    <div className="space-y-2">
                      <label className="text-sm font-medium text-foreground">
                        Chat ID{" "}
                        <span className="text-foreground-muted font-normal">
                          (optional — send a message to your bot to get this)
                        </span>
                      </label>
                      <Input
                        value={state.telegramChatId}
                        onChange={(e) => update("telegramChatId", e.target.value)}
                        placeholder="987654321"
                      />
                    </div>
                  </div>
                )}
              </CardContent>
            </Card>

            {/* WhatsApp */}
            <Card className="opacity-60">
              <CardContent className="py-4">
                <div className="flex items-center gap-4">
                  <div className="flex items-center justify-center w-12 h-12 rounded-xl bg-green-500/10">
                    <MessageCircle className="w-6 h-6 text-accent-success" />
                  </div>
                  <div className="flex-1">
                    <h3 className="font-medium text-foreground">WhatsApp</h3>
                    <p className="text-sm text-foreground-muted">
                      Connect via Twilio WhatsApp Business API.
                    </p>
                  </div>
                  <Badge variant="outline">Configure Later</Badge>
                </div>
              </CardContent>
            </Card>
          </div>
        )}

        {/* Complete */}
        {currentStep.id === "complete" && (
          <div className="text-center py-8">
            <div className="flex items-center justify-center w-20 h-20 rounded-2xl bg-accent-success/10 mx-auto mb-6">
              <Check className="w-10 h-10 text-accent-success" />
            </div>
            <h2 className="text-3xl font-bold text-foreground mb-3">
              You&apos;re All Set!
            </h2>
            <p className="text-foreground-secondary text-lg mb-8 max-w-md mx-auto">
              Your agent <strong>{state.agentName}</strong> is ready to go.
              Start chatting or configure more integrations from the Settings page.
            </p>

            <Card className="text-left max-w-md mx-auto mb-8">
              <CardContent className="py-4 space-y-3">
                <div className="flex items-center justify-between">
                  <span className="text-sm text-foreground-muted">LLM Provider</span>
                  <Badge variant="outline">
                    {state.provider} / {state.model}
                  </Badge>
                </div>
                <div className="flex items-center justify-between">
                  <span className="text-sm text-foreground-muted">Agent Name</span>
                  <span className="text-sm font-medium text-foreground">
                    {state.agentName}
                  </span>
                </div>
                {state.userName && (
                  <div className="flex items-center justify-between">
                    <span className="text-sm text-foreground-muted">User</span>
                    <span className="text-sm font-medium text-foreground">
                      {state.userName}
                    </span>
                  </div>
                )}
                {state.telegramToken && (
                  <div className="flex items-center justify-between">
                    <span className="text-sm text-foreground-muted">Telegram</span>
                    <Badge variant="success">Connected</Badge>
                  </div>
                )}
              </CardContent>
            </Card>

            <Button size="lg" onClick={handleFinish} className="px-8">
              <MessageCircle className="w-4 h-4 mr-2" />
              Start Chatting
            </Button>
          </div>
        )}
      </div>

      {/* Navigation buttons */}
      {currentStep.id !== "welcome" && currentStep.id !== "complete" && (
        <div className="w-full max-w-2xl flex items-center justify-between mt-8 pt-4 border-t border-border">
          <Button variant="ghost" onClick={handlePrev}>
            <ChevronLeft className="w-4 h-4 mr-1" />
            Back
          </Button>

          <div className="flex items-center gap-3">
            {currentStep.id === "integrations" && (
              <Button variant="ghost" onClick={handleNext}>
                Skip for now
              </Button>
            )}
            <Button onClick={handleNext} disabled={saving}>
              {saving ? (
                <Loader2 className="w-4 h-4 mr-1 animate-spin" />
              ) : null}
              {currentStep.id === "integrations" ? "Finish" : "Next"}
              <ChevronRight className="w-4 h-4 ml-1" />
            </Button>
          </div>
        </div>
      )}
    </div>
  );
}
