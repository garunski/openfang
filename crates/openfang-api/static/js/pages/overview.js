// OpenFang Overview Dashboard — Landing page with system stats
'use strict';

function overviewPage() {
  return {
    health: {},
    status: {},
    channels: [],
    projects: [],
    projectsLoading: false,
    mcpServers: [],
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
          this.loadProjects(),
          this.loadChannels(),
          this.loadMcpServers()
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
          this.loadProjects(),
          this.loadChannels(),
          this.loadMcpServers()
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

    async loadMcpServers() {
      try {
        var data = await OpenFangAPI.get('/api/mcp/servers');
        this.mcpServers = data.servers || [];
      } catch(e) { this.mcpServers = []; }
    },

    truncatePath(path, maxLen) {
      if (!path) return '';
      var n = maxLen || 64;
      if (path.length <= n) return path;
      return '\u2026' + path.slice(-(n - 1));
    },

  };
}
