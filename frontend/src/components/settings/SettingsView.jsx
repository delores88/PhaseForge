import {
  CheckCircle2,
  ChevronDown,
  CircleAlert,
  Eye,
  EyeOff,
  KeyRound,
  ListRestart,
  LockKeyhole,
  RefreshCw,
  Save,
  ShieldCheck,
  Trash2,
} from "lucide-react";
import { useCallback, useEffect, useMemo, useState } from "react";

import { ErrorState, LoadingState } from "@/components/shared/AsyncState";
import { api } from "@/lib/api";

const labels = {
  open_ai: { name: "OpenAI", keyPlaceholder: "sk-…" },
  anthropic: { name: "Anthropic", keyPlaceholder: "sk-ant-…" },
};

function emptyForm(status = {}) {
  return {
    apiKey: "",
    baseUrl: status.base_url || "",
    selectedModel: status.model || "",
    customModel: "",
    useCustomModel: false,
  };
}

export default function SettingsView({ backend, onHardware, onEventState }) {
  const [providers, setProviders] = useState([]);
  const [forms, setForms] = useState({});
  const [modelLists, setModelLists] = useState({});
  const [visible, setVisible] = useState({});
  const [busy, setBusy] = useState({});
  const [messages, setMessages] = useState({});
  const [error, setError] = useState(null);
  const [loading, setLoading] = useState(true);

  const update = useCallback((provider, key, value) => {
    setForms((current) => ({
      ...current,
      [provider]: {
        ...(current[provider] || emptyForm()),
        [key]: value,
      },
    }));
  }, []);

  const setProviderBusy = (provider, action = "") => {
    setBusy((current) => ({ ...current, [provider]: action }));
  };

  const notify = (provider, text, tone = "info") => {
    setMessages((current) => ({ ...current, [provider]: { text, tone } }));
  };

  const replaceStatus = useCallback((updated) => {
    setProviders((current) =>
      current.map((item) =>
        item.provider === updated.provider ? updated : item,
      ),
    );
    setForms((current) => ({
      ...current,
      [updated.provider]: {
        ...(current[updated.provider] || emptyForm(updated)),
        apiKey: "",
        baseUrl: updated.base_url,
        selectedModel:
          updated.model || current[updated.provider]?.selectedModel || "",
      },
    }));
  }, []);

  const load = useCallback(async () => {
    if (!backend.connected) return;
    try {
      setError(null);
      const [providerResponse, hardware] = await Promise.all([
        api.providers(),
        api.hardware(),
      ]);
      const values = providerResponse.providers || [];
      setProviders(values);
      setForms((current) => {
        const next = { ...current };
        values.forEach((provider) => {
          next[provider.provider] = {
            ...emptyForm(provider),
            ...(current[provider.provider] || {}),
            apiKey: "",
            baseUrl: provider.base_url,
            selectedModel:
              current[provider.provider]?.selectedModel || provider.model || "",
          };
        });
        return next;
      });
      onHardware?.(hardware);
    } catch (loadError) {
      setError(loadError.message);
    } finally {
      setLoading(false);
    }
  }, [backend.connected, onHardware]);

  useEffect(() => {
    load();
  }, [load]);

  useEffect(() => {
    onEventState?.(backend.connected ? "http" : "disconnected");
  }, [backend.connected, onEventState]);

  const loadModels = useCallback(
    async (provider, quiet = false) => {
      setProviderBusy(provider, "models");
      if (!quiet) notify(provider, "Reading account-visible models…");
      try {
        const response = await api.providerModels(provider);
        const models = response.models || [];
        setModelLists((current) => ({ ...current, [provider]: models }));
        setForms((current) => {
          const form = current[provider] || emptyForm();
          const currentModel = response.current_model || form.selectedModel;
          const firstRecommended = models.find((item) => item.recommended)?.id;
          const fallback = models[0]?.id || "";
          const selectedModel =
            currentModel && models.some((item) => item.id === currentModel)
              ? currentModel
              : firstRecommended || fallback || currentModel;
          return {
            ...current,
            [provider]: { ...form, selectedModel },
          };
        });
        if (!quiet) {
          notify(
            provider,
            models.length
              ? `Loaded ${models.length} model${models.length === 1 ? "" : "s"} visible to this key.`
              : "The provider returned no models. Use Custom model ID if this endpoint accepts one.",
            models.length ? "success" : "warning",
          );
        }
        return models;
      } catch (value) {
        notify(provider, value.message, "error");
        return [];
      } finally {
        setProviderBusy(provider);
      }
    },
    [],
  );

  const saveKey = async (provider) => {
    const form = forms[provider] || emptyForm();
    setProviderBusy(provider, "key");
    notify(provider, "Saving the credential behind the Rust backend…");
    try {
      const updated = await api.saveProviderKey(provider, {
        api_key: form.apiKey,
        base_url: form.baseUrl || null,
      });
      replaceStatus(updated);
      notify(
        provider,
        "Credential saved securely. Loading the models available to this account…",
        "success",
      );
      await loadModels(provider, true);
      notify(
        provider,
        "Credential saved. Choose a model and save that selection separately.",
        "success",
      );
    } catch (value) {
      notify(provider, value.message, "error");
    } finally {
      setProviderBusy(provider);
    }
  };

  const saveModel = async (provider) => {
    const form = forms[provider] || emptyForm();
    const model = form.useCustomModel
      ? form.customModel.trim()
      : form.selectedModel.trim();
    if (!model) {
      notify(provider, "Choose a model or enter a custom model ID.", "error");
      return;
    }
    setProviderBusy(provider, "model");
    try {
      const updated = await api.saveProviderModel(provider, model);
      replaceStatus(updated);
      notify(provider, `Active model saved: ${updated.model}`, "success");
    } catch (value) {
      notify(provider, value.message, "error");
    } finally {
      setProviderBusy(provider);
    }
  };

  const test = async (provider) => {
    setProviderBusy(provider, "test");
    notify(provider, "Testing the saved key and active model…");
    try {
      const response = await api.testProvider(provider);
      notify(
        provider,
        response.message || "Provider connection verified.",
        "success",
      );
    } catch (value) {
      notify(provider, value.message, "error");
    } finally {
      setProviderBusy(provider);
    }
  };

  const removeKey = async (provider) => {
    setProviderBusy(provider, "remove");
    try {
      const updated = await api.deleteProviderKey(provider);
      replaceStatus(updated);
      setModelLists((current) => ({ ...current, [provider]: [] }));
      notify(
        provider,
        "Credential removed. The model preference was retained for later.",
        "success",
      );
    } catch (value) {
      notify(provider, value.message, "error");
    } finally {
      setProviderBusy(provider);
    }
  };

  if (loading && backend.connected) {
    return <LoadingState label="Loading local settings…" />;
  }
  if (error) return <ErrorState message={error} onRetry={load} />;

  return (
    <section className="settingsView settingsView--providersV3">
      <div className="securityCallout">
        <LockKeyhole size={20} />
        <div>
          <strong>Credential storage no longer depends on model selection</strong>
          <p>
            Save a key first. PhaseForge then asks the provider for models visible
            to that account. The model choice is a separate reversible setting,
            so an empty model field can never block secure credential storage.
          </p>
        </div>
      </div>

      <div className="providerGrid providerGrid--wide">
        {providers.map((provider) => (
          <ProviderCard
            key={provider.provider}
            provider={provider}
            metadata={labels[provider.provider] || {
              name: provider.provider,
              keyPlaceholder: "API key",
            }}
            form={forms[provider.provider] || emptyForm(provider)}
            models={modelLists[provider.provider] || []}
            visible={Boolean(visible[provider.provider])}
            busy={busy[provider.provider] || ""}
            message={messages[provider.provider]}
            onUpdate={(key, value) => update(provider.provider, key, value)}
            onToggleVisible={() =>
              setVisible((current) => ({
                ...current,
                [provider.provider]: !current[provider.provider],
              }))
            }
            onSaveKey={() => saveKey(provider.provider)}
            onLoadModels={() => loadModels(provider.provider)}
            onSaveModel={() => saveModel(provider.provider)}
            onTest={() => test(provider.provider)}
            onRemoveKey={() => removeKey(provider.provider)}
          />
        ))}
      </div>

      <section className="surfacePanel settingsPolicy">
        <header className="surfacePanel__header">
          <div>
            <h2>Agent execution boundary</h2>
            <p>Models shape research objects; the trusted runtime controls execution.</p>
          </div>
          <ShieldCheck size={22} />
        </header>
        <div className="boundaryGrid">
          <div>
            <strong>Allowed now</strong>
            <ul>
              <li>Author and revise bounded numerical manifests.</li>
              <li>Inspect normalized molecular structures and deterministic diagnostics.</li>
              <li>Plan candidate QM/MM regions and engine-aware computational campaigns.</li>
              <li>Challenge assumptions and propose falsification work.</li>
            </ul>
          </div>
          <div>
            <strong>Still blocked</strong>
            <ul>
              <li>Arbitrary host-code, shell, or unrestricted filesystem execution.</li>
              <li>Credential access from the browser or experiment sandbox.</li>
              <li>Invented docking, dynamics, quantum, synthesis, safety, or efficacy results.</li>
              <li>Mutation of the trusted laboratory kernel.</li>
            </ul>
          </div>
        </div>
      </section>
    </section>
  );
}

function ProviderCard({
  provider,
  metadata,
  form,
  models,
  visible,
  busy,
  message,
  onUpdate,
  onToggleVisible,
  onSaveKey,
  onLoadModels,
  onSaveModel,
  onTest,
  onRemoveKey,
}) {
  const recommended = useMemo(
    () => models.filter((model) => model.recommended),
    [models],
  );
  const other = useMemo(
    () => models.filter((model) => !model.recommended),
    [models],
  );
  const selectedModel = form.useCustomModel
    ? form.customModel
    : form.selectedModel;
  const working = Boolean(busy);

  return (
    <article className="providerCard providerCard--workflow">
      <header>
        <div className="providerCard__mark">
          <KeyRound size={18} />
        </div>
        <div>
          <h2>{metadata.name}</h2>
          <div className="providerStateRow">
            <span className={provider.key_configured ? "configured" : ""}>
              {provider.key_configured ? (
                <CheckCircle2 size={13} />
              ) : (
                <CircleAlert size={13} />
              )}
              Key {provider.key_configured ? "saved" : "not saved"}
            </span>
            <span className={provider.model_configured ? "configured" : ""}>
              {provider.model_configured ? (
                <CheckCircle2 size={13} />
              ) : (
                <CircleAlert size={13} />
              )}
              Model {provider.model_configured ? "selected" : "not selected"}
            </span>
          </div>
        </div>
      </header>

      <section className="providerStep">
        <div className="providerStep__number">1</div>
        <div className="providerStep__body">
          <div className="providerStep__heading">
            <div>
              <strong>Save the API credential</strong>
              <small>No model ID is required for this step.</small>
            </div>
            {provider.key_configured && (
              <span className="stepComplete">
                <CheckCircle2 size={13} /> Saved
              </span>
            )}
          </div>

          <label>
            <span>API key</span>
            <div className="secretInput">
              <input
                type={visible ? "text" : "password"}
                value={form.apiKey || ""}
                onChange={(event) => onUpdate("apiKey", event.target.value)}
                placeholder={
                  provider.key_configured
                    ? "Paste only to replace the saved key"
                    : metadata.keyPlaceholder
                }
                autoComplete="off"
                spellCheck="false"
              />
              <button
                type="button"
                className="iconButton"
                onClick={onToggleVisible}
                aria-label="Toggle key visibility"
              >
                {visible ? <EyeOff size={15} /> : <Eye size={15} />}
              </button>
            </div>
          </label>

          <details className="providerAdvanced">
            <summary>Advanced endpoint</summary>
            <label>
              <span>Base URL</span>
              <input
                value={form.baseUrl || ""}
                onChange={(event) => onUpdate("baseUrl", event.target.value)}
                placeholder={provider.base_url}
              />
            </label>
            <small>
              Use HTTPS except for a loopback-compatible endpoint. Never send a
              real provider key to an untrusted proxy.
            </small>
          </details>

          <div className="providerStep__actions">
            {provider.key_configured && (
              <button
                type="button"
                className="button button--dangerGhost"
                onClick={onRemoveKey}
                disabled={working}
              >
                <Trash2 size={14} /> Remove key
              </button>
            )}
            <button
              type="button"
              className="button button--primary"
              onClick={onSaveKey}
              disabled={working || !form.apiKey?.trim()}
            >
              <Save size={14} />
              {provider.key_configured ? "Replace key securely" : "Save key securely"}
            </button>
          </div>
        </div>
      </section>

      <section
        className={`providerStep ${
          provider.key_configured ? "" : "providerStep--disabled"
        }`}
      >
        <div className="providerStep__number">2</div>
        <div className="providerStep__body">
          <div className="providerStep__heading">
            <div>
              <strong>Choose the active research model</strong>
              <small>
                PhaseForge loads live account-visible models after the key is saved.
              </small>
            </div>
            {provider.model_configured && (
              <span className="catalogSource catalogSource--live">
                {provider.model}
              </span>
            )}
          </div>

          <div className="modelToolbar">
            <button
              type="button"
              className="button button--secondary"
              onClick={onLoadModels}
              disabled={working || !provider.key_configured}
            >
              <ListRestart size={14} />
              {models.length ? "Refresh models" : "Load models"}
            </button>
            <span>
              {models.length
                ? `${models.length} account-visible models`
                : "No model list loaded"}
            </span>
          </div>

          <label>
            <span>Model</span>
            <div className="selectWrap">
              <select
                value={form.useCustomModel ? "__custom__" : form.selectedModel || ""}
                onChange={(event) => {
                  const custom = event.target.value === "__custom__";
                  onUpdate("useCustomModel", custom);
                  if (!custom) onUpdate("selectedModel", event.target.value);
                }}
                disabled={!provider.key_configured || busy === "models"}
              >
                <option value="" disabled>
                  {provider.key_configured
                    ? "Load models, then choose one"
                    : "Save a key first"}
                </option>
                {recommended.length > 0 && (
                  <optgroup label="Recommended available models">
                    {recommended.map((model) => (
                      <option value={model.id} key={model.id}>
                        {model.display_name || model.id}
                      </option>
                    ))}
                  </optgroup>
                )}
                {other.length > 0 && (
                  <optgroup label="Other account-visible models">
                    {other.map((model) => (
                      <option value={model.id} key={model.id}>
                        {model.display_name || model.id}
                      </option>
                    ))}
                  </optgroup>
                )}
                {provider.model &&
                  !models.some((item) => item.id === provider.model) && (
                    <option value={provider.model}>{provider.model} · current</option>
                  )}
                <option value="__custom__">Custom model ID…</option>
              </select>
              <ChevronDown size={15} />
            </div>
          </label>

          {form.useCustomModel && (
            <label>
              <span>Custom model ID</span>
              <input
                value={form.customModel || ""}
                onChange={(event) => onUpdate("customModel", event.target.value)}
                placeholder="Exact provider model ID or compatible endpoint alias"
                autoComplete="off"
                spellCheck="false"
              />
            </label>
          )}

          <div className="providerStep__actions">
            <button
              type="button"
              className="button button--secondary"
              onClick={onTest}
              disabled={working || !provider.configured}
            >
              <RefreshCw size={14} /> Test complete setup
            </button>
            <button
              type="button"
              className="button button--primary"
              onClick={onSaveModel}
              disabled={
                working ||
                !provider.key_configured ||
                !selectedModel?.trim()
              }
            >
              <Save size={14} /> Save active model
            </button>
          </div>
        </div>
      </section>

      {message?.text && (
        <div className={`providerMessage providerMessage--${message.tone}`}>
          {message.tone === "error" ? (
            <CircleAlert size={15} />
          ) : (
            <CheckCircle2 size={15} />
          )}
          <span>{message.text}</span>
        </div>
      )}
    </article>
  );
}
