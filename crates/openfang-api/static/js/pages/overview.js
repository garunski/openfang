// OpenFang Overview Dashboard — Landing page with system stats + provider status
'use strict';

function overviewPage() {
  return {
    health: {},
    status: {},
    usageSummary: {},
    channels: [],
    projects: [],
    projectsLoading: false,
    providers: [],
    mcpServers: [],
    skillCount: 0,
    loading: true,
    loadError: '',
    refreshTimer: null,
    lastRefresh: null,

    async loadOverview() {
      this.loading = true;
      this.loadError = '';
      try {
        await Promise.all([
          this.loadHealth(),
          this.loadStatus(),
          this.loadUsage(),
          this.loadProjects(),
          this.loadChannels(),
          this.loadProviders(),
          this.loadMcpServers(),
          this.loadSkills()
        ]);
        this.lastRefresh = Date.now();
      } catch(e) {
        this.loadError = e.message || 'Could not load overview data.';
      }
      this.loading = false;
    },

    async loadData() { return this.loadOverview(); },

    navigate(target) {
      var root = typeof Alpine !== 'undefined' ? Alpine.$data(document.body) : null;
      if (root && typeof root.navigate === 'function') {
        root.navigate(target);
      } else {
        var path = String(target == null ? 'overview' : target).replace(/^#\/?/, '').trim() || 'overview';
        window.location.hash = path;
      }
    },

    // Silent background refresh (no loading spinner)
    async silentRefresh() {
      try {
        await Promise.all([
          this.loadHealth(),
          this.loadStatus(),
          this.loadUsage(),
          this.loadProjects(),
          this.loadChannels(),
          this.loadProviders(),
          this.loadMcpServers(),
          this.loadSkills()
        ]);
        this.lastRefresh = Date.now();
      } catch(e) { /* silent */ }
    },

    startAutoRefresh() {
      this.stopAutoRefresh();
      this.refreshTimer = setInterval(() => this.silentRefresh(), 30000);
    },

    stopAutoRefresh() {
      if (this.refreshTimer) {
        clearInterval(this.refreshTimer);
        this.refreshTimer = null;
      }
    },

    async loadHealth() {
      try {
        this.health = await OpenFangAPI.get('/api/health');
      } catch(e) { this.health = { status: 'unreachable' }; }
    },

    async loadStatus() {
      try {
        this.status = await OpenFangAPI.get('/api/status');
      } catch(e) { this.status = {}; throw e; }
    },

    async loadUsage() {
      try {
        var data = await OpenFangAPI.get('/api/usage');
        var agents = data.agents || [];
        var totalTokens = 0;
        var totalTools = 0;
        var totalCost = 0;
        agents.forEach(function(a) {
          totalTokens += (a.total_tokens || 0);
          totalTools += (a.tool_calls || 0);
          totalCost += (a.cost_usd || 0);
        });
        this.usageSummary = {
          total_tokens: totalTokens,
          total_tools: totalTools,
          total_cost: totalCost,
          agent_count: agents.length
        };
      } catch(e) {
        this.usageSummary = { total_tokens: 0, total_tools: 0, total_cost: 0, agent_count: 0 };
      }
    },

    async loadProjects() {
      this.projectsLoading = true;
      try {
        var list = await OpenFangAPI.get('/api/projects');
        this.projects = Array.isArray(list) ? list : [];
        if (typeof Alpine !== 'undefined' && Alpine.store('app')) {
          try {
            var root = Alpine.$data(document.body);
            if (root && typeof root.refreshSidebarProjects === 'function') {
              root.refreshSidebarProjects();
            }
          } catch (e2) { /* ignore */ }
        }
      } catch (e) {
        this.projects = [];
      }
      this.projectsLoading = false;
    },

    deleteProject(p) {
      if (!p || !p.id) return;
      var name = p.name || p.id;
      var self = this;
      if (typeof OpenFangToast !== 'undefined' && OpenFangToast.confirm) {
        OpenFangToast.confirm(
          'Delete project',
          'Remove "' + name + '" from OpenFang? This does not delete files on disk.',
          async function () {
            try {
              await OpenFangAPI.delete('/api/projects/' + encodeURIComponent(p.id));
              OpenFangToast.success('Project removed');
              await self.loadProjects();
              if (typeof Alpine !== 'undefined') {
                var root = Alpine.$data(document.body);
                if (root && typeof root.refreshSidebarProjects === 'function') {
                  root.refreshSidebarProjects();
                }
              }
            } catch (err) {
              OpenFangToast.error(err.message || 'Delete failed');
            }
          }
        );
      } else if (window.confirm('Remove project "' + name + '"?')) {
        OpenFangAPI.delete('/api/projects/' + encodeURIComponent(p.id))
          .then(function () {
            return self.loadProjects();
          })
          .then(function () {
            if (typeof Alpine !== 'undefined') {
              var root = Alpine.$data(document.body);
              if (root && typeof root.refreshSidebarProjects === 'function') {
                root.refreshSidebarProjects();
              }
            }
          })
          .catch(function (err) {
            alert(err.message || 'Delete failed');
          });
      }
    },

    async loadChannels() {
      try {
        var data = await OpenFangAPI.get('/api/channels');
        this.channels = (data.channels || []).filter(function(ch) { return ch.has_token; });
      } catch(e) { this.channels = []; }
    },

    async loadProviders() {
      try {
        var data = await OpenFangAPI.get('/api/providers');
        this.providers = data.providers || [];
      } catch(e) { this.providers = []; }
    },

    async loadMcpServers() {
      try {
        var data = await OpenFangAPI.get('/api/mcp/servers');
        this.mcpServers = data.servers || [];
      } catch(e) { this.mcpServers = []; }
    },

    async loadSkills() {
      try {
        var data = await OpenFangAPI.get('/api/skills');
        this.skillCount = (data.skills || []).length;
      } catch(e) { this.skillCount = 0; }
    },

    get configuredProviders() {
      return this.providers.filter(function(p) { return p.auth_status === 'configured'; });
    },

    get unconfiguredProviders() {
      return this.providers.filter(function(p) { return p.auth_status === 'not_set' || p.auth_status === 'missing'; });
    },

    get connectedMcp() {
      return this.mcpServers.filter(function(s) { return s.status === 'connected'; });
    },

    // Provider health badge color
    providerBadgeClass(p) {
      if (p.auth_status === 'configured') {
        if (p.health === 'cooldown' || p.health === 'open') return 'badge-warn';
        return 'badge-success';
      }
      if (p.auth_status === 'not_set' || p.auth_status === 'missing') return 'badge-muted';
      return 'badge-dim';
    },

    // Provider health tooltip
    providerTooltip(p) {
      if (p.health === 'cooldown') return p.display_name + ' \u2014 cooling down (rate limited)';
      if (p.health === 'open') return p.display_name + ' \u2014 circuit breaker open';
      if (p.auth_status === 'configured') return p.display_name + ' \u2014 ready';
      return p.display_name + ' \u2014 not configured';
    },

    truncatePath(path, maxLen) {
      if (!path) return '';
      var n = maxLen || 64;
      if (path.length <= n) return path;
      return '\u2026' + path.slice(-(n - 1));
    },

    formatUptime(secs) {
      if (!secs) return '-';
      var d = Math.floor(secs / 86400);
      var h = Math.floor((secs % 86400) / 3600);
      var m = Math.floor((secs % 3600) / 60);
      if (d > 0) return d + 'd ' + h + 'h';
      if (h > 0) return h + 'h ' + m + 'm';
      return m + 'm';
    },

    formatNumber(n) {
      if (!n) return '0';
      if (n >= 1000000) return (n / 1000000).toFixed(1) + 'M';
      if (n >= 1000) return (n / 1000).toFixed(1) + 'K';
      return String(n);
    },

    formatCost(n) {
      if (!n || n === 0) return '$0.00';
      if (n < 0.01) return '<$0.01';
      return '$' + n.toFixed(2);
    }
  };
}
