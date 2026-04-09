// OpenFang Settings Page — Provider Hub, Model Catalog, Config, Tools + Security, Network, Migration tabs
'use strict';

function settingsPage() {
  return {
    tab: 'providers',
    sysInfo: {},
    usageData: [],
    tools: [],
    config: {},
    providers: [],
    models: [],
    toolSearch: '',
    modelSearch: '',
    modelProviderFilter: '',
    modelTierFilter: '',
    /** When true, Model Catalog loads GET /api/models?available=true */
    modelsCatalogAvailableOnly: true,
    defaultsOrchestratorProvider: '',
    defaultsOrchestratorModel: '',
    defaultsMemoryEmbeddingProvider: '',
    defaultsMemoryEmbeddingModel: '',
    defaultsSaving: false,
    showCustomModelForm: false,
    customModelId: '',
    customModelProvider: 'openrouter',
    customModelContext: 128000,
    customModelMaxOutput: 8192,
    customModelStatus: '',
    providerKeyInputs: {},
    providerUrlInputs: {},
    providerUrlSaving: {},
    providerTesting: {},
    providerTestResults: {},
    providerEnabledSaving: {},
    modelDropdownSaving: false,
    modelDropdownSaveTimer: null,
    copilotOAuth: { polling: false, userCode: '', verificationUri: '', pollId: '', interval: 5 },
    customProviderName: '',
    customProviderUrl: '',
    customProviderKey: '',
    customProviderStatus: '',
    addingCustomProvider: false,
    loading: true,
    loadError: '',

    // -- Dynamic config state --
    configSchema: null,
    configValues: {},
    configDirty: {},
    configSaving: {},

    // -- Security state --
    securityData: null,
    secLoading: false,
    verifyingChain: false,
    chainResult: null,

    coreFeatures: [
      {
        name: 'Path Traversal Prevention', key: 'path_traversal',
        description: 'Blocks directory escape attacks (../) in all file operations. Two-phase validation: syntactic rejection of path components, then canonicalization to normalize symlinks.',
        threat: 'Directory escape, privilege escalation via symlinks',
        impl: 'host_functions.rs — safe_resolve_path() + safe_resolve_parent()'
      },
      {
        name: 'SSRF Protection', key: 'ssrf_protection',
        description: 'Blocks outbound requests to private IPs, localhost, and cloud metadata endpoints (AWS/GCP/Azure). Validates DNS resolution results to defeat rebinding attacks.',
        threat: 'Internal network reconnaissance, cloud credential theft',
        impl: 'host_functions.rs — is_ssrf_target() + is_private_ip()'
      },
      {
        name: 'Capability-Based Access Control', key: 'capability_system',
        description: 'Deny-by-default permission system. Every agent operation (file I/O, network, shell, memory, spawn) requires an explicit capability grant in the manifest.',
        threat: 'Unauthorized resource access, sandbox escape',
        impl: 'host_functions.rs — check_capability() on every host function'
      },
      {
        name: 'Privilege Escalation Prevention', key: 'privilege_escalation_prevention',
        description: 'When a parent agent spawns a child, the kernel enforces child capabilities are a subset of parent capabilities. No agent can grant rights it does not have.',
        threat: 'Capability escalation through agent spawning chains',
        impl: 'kernel_handle.rs — spawn_agent_checked()'
      },
      {
        name: 'Subprocess Environment Isolation', key: 'subprocess_isolation',
        description: 'Child processes (shell tools) inherit only a safe allow-list of environment variables. API keys, database passwords, and secrets are never leaked to subprocesses.',
        threat: 'Secret exfiltration via child process environment',
        impl: 'subprocess_sandbox.rs — env_clear() + SAFE_ENV_VARS'
      },
      {
        name: 'Security Headers', key: 'security_headers',
        description: 'Every HTTP response includes CSP, X-Frame-Options: DENY, X-Content-Type-Options: nosniff, Referrer-Policy, and X-XSS-Protection headers.',
        threat: 'XSS, clickjacking, MIME sniffing, content injection',
        impl: 'middleware.rs — security_headers()'
      },
      {
        name: 'Wire Protocol Authentication', key: 'wire_hmac_auth',
        description: 'Agent-to-agent OFP connections use HMAC-SHA256 mutual authentication with nonce-based handshake and constant-time signature comparison (subtle crate).',
        threat: 'Man-in-the-middle attacks on mesh network',
        impl: 'peer.rs — hmac_sign() + hmac_verify()'
      },
      {
        name: 'Request ID Tracking', key: 'request_id_tracking',
        description: 'Every API request receives a unique UUID (x-request-id header) and is logged with method, path, status code, and latency for full traceability.',
        threat: 'Untraceable actions, forensic blind spots',
        impl: 'middleware.rs — request_logging()'
      }
    ],

    configurableFeatures: [
      {
        name: 'API Rate Limiting', key: 'rate_limiter',
        description: 'GCRA (Generic Cell Rate Algorithm) with cost-aware tokens. Different endpoints cost different amounts — spawning an agent costs 50 tokens, health check costs 1.',
        configHint: 'Hard-coded: 500 tokens/minute per IP. Edit rate_limiter.rs to tune.',
        valueKey: 'rate_limiter'
      },
      {
        name: 'WebSocket Connection Limits', key: 'websocket_limits',
        description: 'Per-IP connection cap prevents connection exhaustion. Idle timeout closes abandoned connections. Message rate limiting prevents flooding.',
        configHint: 'Hard-coded: 5 connections/IP, 30min idle timeout, 64KB max message. Edit ws.rs to tune.',
        valueKey: 'websocket_limits'
      },
      {
        name: 'WASM Dual Metering', key: 'wasm_sandbox',
        description: 'WASM modules run with two independent resource limits: fuel metering (CPU instruction count) and epoch interruption (wall-clock timeout with watchdog thread).',
        configHint: 'Default: 1M fuel units, 30s timeout. Configurable per-agent via SandboxConfig.',
        valueKey: 'wasm_sandbox'
      },
      {
        name: 'Bearer Token Authentication', key: 'auth',
        description: 'All non-health endpoints require Authorization: Bearer header. When no API key is configured, all requests are restricted to localhost only.',
        configHint: 'Set api_key in ~/.openfang/config.toml for remote access. Empty = localhost only.',
        valueKey: 'auth'
      }
    ],

    monitoringFeatures: [
      {
        name: 'Merkle Audit Trail', key: 'audit_trail',
        description: 'Every security-critical action is appended to an immutable, tamper-evident log. Each entry is cryptographically linked to the previous via SHA-256 hash chain.',
        configHint: 'Always active. Verify chain integrity from the Audit Log page.',
        valueKey: 'audit_trail'
      },
      {
        name: 'Information Flow Taint Tracking', key: 'taint_tracking',
        description: 'Labels data by provenance (ExternalNetwork, UserInput, PII, Secret, UntrustedAgent) and blocks unsafe flows: external data cannot reach shell_exec, secrets cannot reach network.',
        configHint: 'Always active. Prevents data flow attacks automatically.',
        valueKey: 'taint_tracking'
      },
      {
        name: 'Ed25519 Manifest Signing', key: 'manifest_signing',
        description: 'Agent manifests can be cryptographically signed with Ed25519. Verify manifest integrity before loading to prevent supply chain tampering.',
        configHint: 'Available for use. Sign manifests with ed25519-dalek for verification.',
        valueKey: 'manifest_signing'
      }
    ],

    // -- Peers state --
    peers: [],
    peersLoading: false,
    peersLoadError: '',
    _peerPollTimer: null,

    // -- Settings load --
    async loadSettings() {
      this.loading = true;
      this.loadError = '';
      try {
        await Promise.all([
          this.loadSysInfo(),
          this.loadUsage(),
          this.loadTools(),
          this.loadConfig(),
          this.loadProviders(),
          this.loadModels()
        ]);
      } catch(e) {
        this.loadError = e.message || 'Could not load settings.';
      }
      this.loading = false;
    },

    async loadData() { return this.loadSettings(); },

    async loadSysInfo() {
      try {
        var ver = await OpenFangAPI.get('/api/version');
        var status = await OpenFangAPI.get('/api/status');
        this.sysInfo = {
          version: ver.version || '-',
          platform: ver.platform || '-',
          arch: ver.arch || '-',
          uptime_seconds: status.uptime_seconds || 0,
          agent_count: status.agent_count || 0,
          default_provider: status.default_provider || '-',
          default_model: status.default_model || '-'
        };
      } catch(e) { throw e; }
    },

    async loadUsage() {
      try {
        var data = await OpenFangAPI.get('/api/usage');
        this.usageData = data.agents || [];
      } catch(e) { this.usageData = []; }
    },

    async loadTools() {
      try {
        var data = await OpenFangAPI.get('/api/tools');
        this.tools = data.tools || [];
      } catch(e) { this.tools = []; }
    },

    async loadConfig() {
      try {
        this.config = await OpenFangAPI.get('/api/config');
      } catch(e) { this.config = {}; }
    },

    async loadProviders() {
      try {
        var data = await OpenFangAPI.get('/api/providers');
        this.providers = data.providers || [];
        for (var i = 0; i < this.providers.length; i++) {
          var p = this.providers[i];
          if (p.is_local) {
            if (!this.providerUrlInputs[p.id]) {
              this.providerUrlInputs[p.id] = p.base_url || '';
            }
            if (this.providerUrlSaving[p.id] === undefined) {
              this.providerUrlSaving[p.id] = false;
            }
          }
        }
      } catch(e) { this.providers = []; }
    },

    async loadModels() {
      try {
        var q = this.modelsCatalogAvailableOnly ? '?available=true' : '';
        var data = await OpenFangAPI.get('/api/models' + q);
        this.models = data.models || [];
        for (var i = 0; i < this.models.length; i++) {
          if (this.models[i].show_in_dropdowns === undefined) this.models[i].show_in_dropdowns = true;
        }
      } catch(e) { this.models = []; }
    },

    async setModelsCatalogAvailableOnly(availableOnly) {
      if (this.modelsCatalogAvailableOnly === availableOnly) return;
      this.modelsCatalogAvailableOnly = availableOnly;
      await this.loadModels();
    },

    async openDefaultsTab() {
      this.tab = 'defaults';
      await Promise.all([
        this.refreshDefaultsOrchestratorForm(),
        this.refreshDefaultsMemoryEmbeddingForm(),
      ]);
    },

    async refreshDefaultsOrchestratorForm() {
      try {
        var c = await OpenFangAPI.get('/api/config');
        var o = c.orchestrator_default_model || {};
        this.defaultsOrchestratorProvider = o.provider != null ? String(o.provider) : '';
        this.defaultsOrchestratorModel = o.model != null ? String(o.model) : '';
        if (!this.models.length) await this.loadModels();
      } catch(e) {
        OpenFangToast.error(e.message || 'Could not load defaults');
      }
    },

    get defaultsOrchestratorModels() {
      var p = (this.defaultsOrchestratorProvider || '').trim();
      return (this.models || []).filter(function(m) {
        if (!m.available) return false;
        if (m.show_in_dropdowns === false) return false;
        return !p || m.provider === p;
      });
    },

    get uniqueDefaultsOrchestratorProviders() {
      var seen = {};
      (this.models || []).forEach(function(m) {
        if (m.provider && m.available && m.show_in_dropdowns !== false) seen[m.provider] = true;
      });
      return Object.keys(seen).sort();
    },

    onDefaultsOrchestratorProviderChange() {
      var self = this;
      var ok = this.defaultsOrchestratorModels.some(function(m) {
        return String(m.id) === String(self.defaultsOrchestratorModel);
      });
      if (!ok) this.defaultsOrchestratorModel = '';
    },

    async saveOrchestratorDefaults() {
      var prov = (this.defaultsOrchestratorProvider || '').trim();
      var mod = (this.defaultsOrchestratorModel || '').trim();
      if ((prov && !mod) || (!prov && mod)) {
        OpenFangToast.error('Choose both a provider and a model, or clear both to use the bundled hand default.');
        return;
      }
      this.defaultsSaving = true;
      try {
        await OpenFangAPI.post('/api/config/set', { path: 'orchestrator_default_model.provider', value: prov });
        await OpenFangAPI.post('/api/config/set', { path: 'orchestrator_default_model.model', value: mod });
        OpenFangToast.success('Saved. New per-project orchestrators pick this model when created.');
        await this.refreshDefaultsOrchestratorForm();
      } catch(e) {
        OpenFangToast.error(e.message || 'Save failed');
      }
      this.defaultsSaving = false;
    },

    async clearOrchestratorDefaults() {
      this.defaultsOrchestratorProvider = '';
      this.defaultsOrchestratorModel = '';
      await this.saveOrchestratorDefaults();
    },

    async refreshDefaultsMemoryEmbeddingForm() {
      try {
        var c = await OpenFangAPI.get('/api/config');
        var m = c.memory_default_embedding || {};
        this.defaultsMemoryEmbeddingProvider = m.provider != null ? String(m.provider) : '';
        this.defaultsMemoryEmbeddingModel = m.model != null ? String(m.model) : '';
        if (!this.models.length) await this.loadModels();
        this.onDefaultsMemoryEmbeddingProviderChange();
      } catch(e) {
        OpenFangToast.error(e.message || 'Could not load memory embedding defaults');
      }
    },

    get defaultsMemoryEmbeddingModels() {
      var p = (this.defaultsMemoryEmbeddingProvider || '').trim();
      return (this.models || []).filter(function(m) {
        if (!m.available) return false;
        if (m.show_in_dropdowns === false) return false;
        return !p || m.provider === p;
      });
    },

    onDefaultsMemoryEmbeddingProviderChange() {
      var self = this;
      var ok = this.defaultsMemoryEmbeddingModels.some(function(m) {
        return String(m.id) === String(self.defaultsMemoryEmbeddingModel);
      });
      if (!ok) this.defaultsMemoryEmbeddingModel = '';
    },

    async saveMemoryEmbeddingDefaults() {
      var prov = (this.defaultsMemoryEmbeddingProvider || '').trim();
      var mod = (this.defaultsMemoryEmbeddingModel || '').trim();
      if ((prov && !mod) || (!prov && mod)) {
        OpenFangToast.error('Choose both a provider and a model from the lists, or clear both to use [memory] in config.toml.');
        return;
      }
      this.defaultsSaving = true;
      try {
        await OpenFangAPI.post('/api/config/set', { path: 'memory_default_embedding.provider', value: prov });
        await OpenFangAPI.post('/api/config/set', { path: 'memory_default_embedding.model', value: mod });
        OpenFangToast.success('Saved. Memory semantic search uses this embedding provider when set.');
        await this.refreshDefaultsMemoryEmbeddingForm();
      } catch(e) {
        OpenFangToast.error(e.message || 'Save failed');
      }
      this.defaultsSaving = false;
    },

    async clearMemoryEmbeddingDefaults() {
      this.defaultsMemoryEmbeddingProvider = '';
      this.defaultsMemoryEmbeddingModel = '';
      await this.saveMemoryEmbeddingDefaults();
    },

    async addCustomModel() {
      var id = this.customModelId.trim();
      if (!id) return;
      this.customModelStatus = 'Adding...';
      try {
        await OpenFangAPI.post('/api/models/custom', {
          id: id,
          provider: this.customModelProvider || 'openrouter',
          context_window: this.customModelContext || 128000,
          max_output_tokens: this.customModelMaxOutput || 8192,
        });
        this.customModelStatus = 'Added!';
        this.customModelId = '';
        this.showCustomModelForm = false;
        await this.loadModels();
      } catch(e) {
        this.customModelStatus = 'Error: ' + (e.message || 'Failed');
      }
    },

    async saveModelDropdownVisibility(opts) {
      var silent = opts && opts.silent === true;
      var merged = {};
      try {
        var cur = await OpenFangAPI.get('/api/models/dropdown-disabled');
        var arr = (cur && cur.disabled_ids) || [];
        for (var j = 0; j < arr.length; j++) merged[String(arr[j])] = true;
      } catch (e) {
        merged = {};
      }
      for (var i = 0; i < (this.models || []).length; i++) {
        var m = this.models[i];
        var id = String(m.id);
        if (m.show_in_dropdowns === false) merged[id] = true;
        else delete merged[id];
      }
      var disabled = Object.keys(merged);
      this.modelDropdownSaving = true;
      try {
        await OpenFangAPI.put('/api/models/dropdown-disabled', { disabled_ids: disabled });
        if (!silent) {
          OpenFangToast.success('Saved dropdown list (' + disabled.length + ' hidden)');
        }
        await this.loadModels();
      } catch (e) {
        OpenFangToast.error('Failed to save: ' + (e.message || String(e)));
      }
      this.modelDropdownSaving = false;
    },

    scheduleModelDropdownSave() {
      var self = this;
      if (this.modelDropdownSaveTimer) clearTimeout(this.modelDropdownSaveTimer);
      this.modelDropdownSaveTimer = setTimeout(function() {
        self.modelDropdownSaveTimer = null;
        self.saveModelDropdownVisibility({ silent: true });
      }, 400);
    },

    toggleModelShowInDropdowns(m) {
      var on = m.show_in_dropdowns !== false;
      m.show_in_dropdowns = !on;
      this.scheduleModelDropdownSave();
    },

    async deleteCustomModel(modelId) {
      if (!confirm('Delete custom model "' + modelId + '"?')) return;
      try {
        await OpenFangAPI.del('/api/models/custom/' + encodeURIComponent(modelId));
        OpenFangToast.success('Model deleted');
        await this.loadModels();
      } catch(e) {
        OpenFangToast.error('Failed to delete: ' + (e.message || 'Unknown error'));
      }
    },

    async loadConfigSchema() {
      try {
        var results = await Promise.all([
          OpenFangAPI.get('/api/config/schema').catch(function() { return {}; }),
          OpenFangAPI.get('/api/config')
        ]);
        this.configSchema = results[0].sections || null;
        this.configValues = results[1] || {};
      } catch(e) { /* silent */ }
    },

    isConfigDirty(section, field) {
      return this.configDirty[section + '.' + field] === true;
    },

    markConfigDirty(section, field) {
      this.configDirty[section + '.' + field] = true;
    },

    async saveConfigField(section, field, value) {
      var key = section + '.' + field;
      // Root-level fields (api_key, api_listen, log_level) use just the field name
      var sectionMeta = this.configSchema && this.configSchema[section];
      var path = (sectionMeta && sectionMeta.root_level) ? field : key;
      this.configSaving[key] = true;
      try {
        await OpenFangAPI.post('/api/config/set', { path: path, value: value });
        this.configDirty[key] = false;
        OpenFangToast.success('Saved ' + field);
      } catch(e) {
        OpenFangToast.error('Failed to save: ' + e.message);
      }
      this.configSaving[key] = false;
    },

    get filteredTools() {
      var q = this.toolSearch.toLowerCase().trim();
      if (!q) return this.tools;
      return this.tools.filter(function(t) {
        return t.name.toLowerCase().indexOf(q) !== -1 ||
               (t.description || '').toLowerCase().indexOf(q) !== -1;
      });
    },

    get filteredModels() {
      var self = this;
      return this.models.filter(function(m) {
        if (self.modelProviderFilter && m.provider !== self.modelProviderFilter) return false;
        if (self.modelTierFilter && m.tier !== self.modelTierFilter) return false;
        if (self.modelSearch) {
          var q = self.modelSearch.toLowerCase();
          if (m.id.toLowerCase().indexOf(q) === -1 &&
              (m.display_name || '').toLowerCase().indexOf(q) === -1) return false;
        }
        return true;
      });
    },

    get uniqueProviderNames() {
      var seen = {};
      this.models.forEach(function(m) { seen[m.provider] = true; });
      return Object.keys(seen).sort();
    },

    get uniqueTiers() {
      var seen = {};
      this.models.forEach(function(m) { if (m.tier) seen[m.tier] = true; });
      return Object.keys(seen).sort();
    },

    providerEnabledBadgeText(p) {
      return p.effective_enabled ? 'Enabled' : 'Not enabled';
    },

    providerEnabledBadgeClass(p) {
      return p.effective_enabled ? 'badge-success' : 'badge-muted';
    },

    providerActiveBadgeText(p) {
      return p.models_usable ? 'Active' : 'Not active';
    },

    providerActiveBadgeClass(p) {
      return p.models_usable ? 'badge-success' : 'badge-muted';
    },

    /** Dashboard key field: only after user turns the provider on (not env-only). */
    showProviderKeyInput(p) {
      if (!p.api_key_env || p.key_required === false) return false;
      if (p.env_key_configured) return false;
      if (!p.settings_enabled) return false;
      return p.auth_status !== 'configured';
    },

    providerCardClass(p) {
      if (p.models_usable) return 'configured';
      if (p.effective_enabled && (p.auth_status === 'not_set' || p.auth_status === 'missing')) return 'not-configured';
      if (p.auth_status === 'not_set' || p.auth_status === 'missing') return 'not-configured';
      return 'no-key';
    },

    tierBadgeClass(tier) {
      if (!tier) return '';
      var t = tier.toLowerCase();
      if (t === 'frontier') return 'tier-frontier';
      if (t === 'smart') return 'tier-smart';
      if (t === 'balanced') return 'tier-balanced';
      if (t === 'fast') return 'tier-fast';
      return '';
    },

    formatCost(cost) {
      if (!cost && cost !== 0) return '-';
      return '$' + cost.toFixed(4);
    },

    formatContext(ctx) {
      if (!ctx) return '-';
      if (ctx >= 1000000) return (ctx / 1000000).toFixed(1) + 'M';
      if (ctx >= 1000) return Math.round(ctx / 1000) + 'K';
      return String(ctx);
    },

    formatUptime(secs) {
      if (!secs) return '-';
      var h = Math.floor(secs / 3600);
      var m = Math.floor((secs % 3600) / 60);
      var s = secs % 60;
      if (h > 0) return h + 'h ' + m + 'm';
      if (m > 0) return m + 'm ' + s + 's';
      return s + 's';
    },

    providerToggleActive(p) {
      return !!(p.settings_enabled || p.env_key_configured);
    },

    providerToggleDisabled(p) {
      return !!p.env_key_configured;
    },

    async toggleProviderEnabled(p) {
      if (p.env_key_configured || this.providerEnabledSaving[p.id]) return;
      var enabled = !p.settings_enabled;
      this.providerEnabledSaving[p.id] = true;
      try {
        await OpenFangAPI.put('/api/providers/' + encodeURIComponent(p.id) + '/enabled', { enabled: enabled });
        OpenFangToast.success((enabled ? 'Enabled ' : 'Disabled ') + p.display_name);
        await this.loadProviders();
        await this.loadModels();
      } catch (e) {
        OpenFangToast.error('Failed to update provider: ' + (e.message || String(e)));
      }
      this.providerEnabledSaving[p.id] = false;
    },

    async saveProviderKey(provider) {
      var key = this.providerKeyInputs[provider.id];
      if (!key || !key.trim()) { OpenFangToast.error('Please enter an API key'); return; }
      try {
        var resp = await OpenFangAPI.post('/api/providers/' + encodeURIComponent(provider.id) + '/key', { key: key.trim() });
        if (resp && resp.switched_default) {
          OpenFangToast.warning(resp.message || 'Default provider was switched to ' + provider.display_name);
        } else {
          OpenFangToast.success('API key saved for ' + provider.display_name);
        }
        this.providerKeyInputs[provider.id] = '';
        await this.loadProviders();
        await this.loadModels();
      } catch(e) {
        OpenFangToast.error('Failed to save key: ' + e.message);
      }
    },

    async removeProviderKey(provider) {
      try {
        await OpenFangAPI.del('/api/providers/' + encodeURIComponent(provider.id) + '/key');
        OpenFangToast.success('API key removed for ' + provider.display_name);
        await this.loadProviders();
        await this.loadModels();
      } catch(e) {
        OpenFangToast.error('Failed to remove key: ' + e.message);
      }
    },

    async startCopilotOAuth() {
      this.copilotOAuth.polling = true;
      this.copilotOAuth.userCode = '';
      try {
        var resp = await OpenFangAPI.post('/api/providers/github-copilot/oauth/start', {});
        this.copilotOAuth.userCode = resp.user_code;
        this.copilotOAuth.verificationUri = resp.verification_uri;
        this.copilotOAuth.pollId = resp.poll_id;
        this.copilotOAuth.interval = resp.interval || 5;
        window.open(resp.verification_uri, '_blank');
        this.pollCopilotOAuth();
      } catch(e) {
        OpenFangToast.error('Failed to start Copilot login: ' + e.message);
        this.copilotOAuth.polling = false;
      }
    },

    pollCopilotOAuth() {
      var self = this;
      setTimeout(async function() {
        if (!self.copilotOAuth.pollId) return;
        try {
          var resp = await OpenFangAPI.get('/api/providers/github-copilot/oauth/poll/' + self.copilotOAuth.pollId);
          if (resp.status === 'complete') {
            OpenFangToast.success('GitHub Copilot authenticated successfully!');
            self.copilotOAuth = { polling: false, userCode: '', verificationUri: '', pollId: '', interval: 5 };
            await self.loadProviders();
            await self.loadModels();
          } else if (resp.status === 'pending') {
            if (resp.interval) self.copilotOAuth.interval = resp.interval;
            self.pollCopilotOAuth();
          } else if (resp.status === 'expired') {
            OpenFangToast.error('Device code expired. Please try again.');
            self.copilotOAuth = { polling: false, userCode: '', verificationUri: '', pollId: '', interval: 5 };
          } else if (resp.status === 'denied') {
            OpenFangToast.error('Access denied by user.');
            self.copilotOAuth = { polling: false, userCode: '', verificationUri: '', pollId: '', interval: 5 };
          } else {
            OpenFangToast.error('OAuth error: ' + (resp.error || resp.status));
            self.copilotOAuth = { polling: false, userCode: '', verificationUri: '', pollId: '', interval: 5 };
          }
        } catch(e) {
          OpenFangToast.error('Poll error: ' + e.message);
          self.copilotOAuth = { polling: false, userCode: '', verificationUri: '', pollId: '', interval: 5 };
        }
      }, self.copilotOAuth.interval * 1000);
    },

    async testProvider(provider) {
      this.providerTesting[provider.id] = true;
      this.providerTestResults[provider.id] = null;
      try {
        var result = await OpenFangAPI.post('/api/providers/' + encodeURIComponent(provider.id) + '/test', {});
        this.providerTestResults[provider.id] = result;
        if (result.status === 'ok') {
          OpenFangToast.success(provider.display_name + ' connected (' + (result.latency_ms || '?') + 'ms)');
        } else {
          OpenFangToast.error(provider.display_name + ': ' + (result.error || 'Connection failed'));
        }
      } catch(e) {
        this.providerTestResults[provider.id] = { status: 'error', error: e.message };
        OpenFangToast.error('Test failed: ' + e.message);
      }
      this.providerTesting[provider.id] = false;
    },

    async saveProviderUrl(provider) {
      var url = this.providerUrlInputs[provider.id];
      if (!url || !url.trim()) { OpenFangToast.error('Please enter a base URL'); return; }
      url = url.trim();
      if (url.indexOf('http://') !== 0 && url.indexOf('https://') !== 0) {
        OpenFangToast.error('URL must start with http:// or https://'); return;
      }
      this.providerUrlSaving[provider.id] = true;
      try {
        var result = await OpenFangAPI.put('/api/providers/' + encodeURIComponent(provider.id) + '/url', { base_url: url });
        if (result.reachable) {
          OpenFangToast.success(provider.display_name + ' URL saved &mdash; reachable (' + (result.latency_ms || '?') + 'ms)');
        } else {
          OpenFangToast.warning(provider.display_name + ' URL saved but not reachable');
        }
        await this.loadProviders();
      } catch(e) {
        OpenFangToast.error('Failed to save URL: ' + e.message);
      }
      this.providerUrlSaving[provider.id] = false;
    },

    async addCustomProvider() {
      var name = this.customProviderName.trim().toLowerCase().replace(/[^a-z0-9-]/g, '-').replace(/-+/g, '-');
      if (!name) { OpenFangToast.error('Please enter a provider name'); return; }
      var url = this.customProviderUrl.trim();
      if (!url) { OpenFangToast.error('Please enter a base URL'); return; }
      if (url.indexOf('http://') !== 0 && url.indexOf('https://') !== 0) {
        OpenFangToast.error('URL must start with http:// or https://'); return;
      }
      this.addingCustomProvider = true;
      this.customProviderStatus = '';
      try {
        var result = await OpenFangAPI.put('/api/providers/' + encodeURIComponent(name) + '/url', { base_url: url });
        if (this.customProviderKey.trim()) {
          await OpenFangAPI.post('/api/providers/' + encodeURIComponent(name) + '/key', { key: this.customProviderKey.trim() });
        }
        this.customProviderName = '';
        this.customProviderUrl = '';
        this.customProviderKey = '';
        this.customProviderStatus = '';
        OpenFangToast.success('Provider "' + name + '" added' + (result.reachable ? ' (reachable)' : ' (not reachable yet)'));
        await this.loadProviders();
      } catch(e) {
        this.customProviderStatus = 'Error: ' + (e.message || 'Failed');
        OpenFangToast.error('Failed to add provider: ' + e.message);
      }
      this.addingCustomProvider = false;
    },

    // -- Security methods --
    async loadSecurity() {
      this.secLoading = true;
      try {
        this.securityData = await OpenFangAPI.get('/api/security');
      } catch(e) {
        this.securityData = null;
      }
      this.secLoading = false;
    },

    isActive(key) {
      if (!this.securityData) return true;
      var core = this.securityData.core_protections || {};
      if (core[key] !== undefined) return core[key];
      return true;
    },

    getConfigValue(key) {
      if (!this.securityData) return null;
      var cfg = this.securityData.configurable || {};
      return cfg[key] || null;
    },

    getMonitoringValue(key) {
      if (!this.securityData) return null;
      var mon = this.securityData.monitoring || {};
      return mon[key] || null;
    },

    formatConfigValue(feature) {
      var val = this.getConfigValue(feature.valueKey);
      if (!val) return feature.configHint;
      switch (feature.valueKey) {
        case 'rate_limiter':
          return 'Algorithm: ' + (val.algorithm || 'GCRA') + ' | ' + (val.tokens_per_minute || 500) + ' tokens/min per IP';
        case 'websocket_limits':
          return 'Max ' + (val.max_per_ip || 5) + ' conn/IP | ' + Math.round((val.idle_timeout_secs || 1800) / 60) + 'min idle timeout | ' + Math.round((val.max_message_size || 65536) / 1024) + 'KB max msg';
        case 'wasm_sandbox':
          return 'Fuel: ' + (val.fuel_metering ? 'ON' : 'OFF') + ' | Epoch: ' + (val.epoch_interruption ? 'ON' : 'OFF') + ' | Timeout: ' + (val.default_timeout_secs || 30) + 's';
        case 'auth':
          return 'Mode: ' + (val.mode || 'unknown') + (val.api_key_set ? ' (key configured)' : ' (no key set)');
        default:
          return feature.configHint;
      }
    },

    formatMonitoringValue(feature) {
      var val = this.getMonitoringValue(feature.valueKey);
      if (!val) return feature.configHint;
      switch (feature.valueKey) {
        case 'audit_trail':
          return (val.enabled ? 'Active' : 'Disabled') + ' | ' + (val.algorithm || 'SHA-256') + ' | ' + (val.entry_count || 0) + ' entries logged';
        case 'taint_tracking':
          var labels = val.tracked_labels || [];
          return (val.enabled ? 'Active' : 'Disabled') + ' | Tracking: ' + labels.join(', ');
        case 'manifest_signing':
          return 'Algorithm: ' + (val.algorithm || 'Ed25519') + ' | ' + (val.available ? 'Available' : 'Not available');
        default:
          return feature.configHint;
      }
    },

    async verifyAuditChain() {
      this.verifyingChain = true;
      this.chainResult = null;
      try {
        var res = await OpenFangAPI.get('/api/audit/verify');
        this.chainResult = res;
      } catch(e) {
        this.chainResult = { valid: false, error: e.message };
      }
      this.verifyingChain = false;
    },

    // -- Peers methods --
    async loadPeers() {
      this.peersLoading = true;
      this.peersLoadError = '';
      try {
        var data = await OpenFangAPI.get('/api/peers');
        this.peers = (data.peers || []).map(function(p) {
          return {
            node_id: p.node_id,
            node_name: p.node_name,
            address: p.address,
            state: p.state,
            agent_count: (p.agents || []).length,
            protocol_version: p.protocol_version || 1
          };
        });
      } catch(e) {
        this.peers = [];
        this.peersLoadError = e.message || 'Could not load peers.';
      }
      this.peersLoading = false;
    },

    startPeerPolling() {
      var self = this;
      this.stopPeerPolling();
      this._peerPollTimer = setInterval(async function() {
        if (self.tab !== 'network') { self.stopPeerPolling(); return; }
        try {
          var data = await OpenFangAPI.get('/api/peers');
          self.peers = (data.peers || []).map(function(p) {
            return {
              node_id: p.node_id,
              node_name: p.node_name,
              address: p.address,
              state: p.state,
              agent_count: (p.agents || []).length,
              protocol_version: p.protocol_version || 1
            };
          });
        } catch(e) { /* silent */ }
      }, 15000);
    },

    stopPeerPolling() {
      if (this._peerPollTimer) { clearInterval(this._peerPollTimer); this._peerPollTimer = null; }
    },

    destroy() {
      this.stopPeerPolling();
    }
  };
}
