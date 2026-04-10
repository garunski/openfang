// OpenFang Logs Page — Real-time log viewer (SSE streaming + polling fallback) + Audit Trail + Files inspector
'use strict';

function logsPage() {
  return {
    tab: 'live',
    // -- Live logs state --
    entries: [],
    levelFilter: '',
    textFilter: '',
    autoRefresh: true,
    hovering: false,
    loading: true,
    loadError: '',
    _pollTimer: null,

    // -- SSE streaming state --
    _eventSource: null,
    streamConnected: false,
    streamPaused: false,

    // -- Audit state --
    auditEntries: [],
    tipHash: '',
    chainValid: null,
    filterAction: '',
    auditLoading: false,
    auditLoadError: '',

    // -- Files (filesystem log inspector) --
    fileList: [],
    fileListLoading: false,
    selectedFileKey: '',
    fileContent: '',
    fileChunkLimit: 262144,
    fileLoadError: '',
    fileRefreshing: false,
    fileFollow: true,
    fileSearch: '',
    fileLevelFilter: '',
    fileViewMode: 'formatted', // 'formatted' | 'raw'
    fileUtf8Lossy: false,
    _filePollTimer: null,

    startStreaming: function() {
      var self = this;
      if (this._eventSource) { this._eventSource.close(); this._eventSource = null; }

      var url = '/api/logs/stream';
      var sep = '?';
      var token = OpenFangAPI.getToken();
      if (token) { url += sep + 'token=' + encodeURIComponent(token); sep = '&'; }

      try {
        this._eventSource = new EventSource(url);
      } catch(e) {
        // EventSource not supported or blocked; fall back to polling
        this.streamConnected = false;
        this.startPolling();
        return;
      }

      this._eventSource.onopen = function() {
        self.streamConnected = true;
        self.loading = false;
        self.loadError = '';
      };

      this._eventSource.onmessage = function(event) {
        if (self.streamPaused) return;
        try {
          var entry = JSON.parse(event.data);
          // Avoid duplicate entries by checking seq
          var dominated = false;
          for (var i = 0; i < self.entries.length; i++) {
            if (self.entries[i].seq === entry.seq) { dominated = true; break; }
          }
          if (!dominated) {
            self.entries.push(entry);
            // Cap at 500 entries (remove oldest)
            if (self.entries.length > 500) {
              self.entries.splice(0, self.entries.length - 500);
            }
            // Auto-scroll to bottom
            if (self.autoRefresh && !self.hovering) {
              self.$nextTick(function() {
                var el = document.getElementById('log-container');
                if (el) el.scrollTop = el.scrollHeight;
              });
            }
          }
        } catch(e) {
          // Ignore parse errors (heartbeat comments are not delivered to onmessage)
        }
      };

      this._eventSource.onerror = function() {
        self.streamConnected = false;
        if (self._eventSource) {
          self._eventSource.close();
          self._eventSource = null;
        }
        // Fall back to polling
        self.startPolling();
      };
    },

    startPolling: function() {
      var self = this;
      this.streamConnected = false;
      this.fetchLogs();
      if (this._pollTimer) clearInterval(this._pollTimer);
      this._pollTimer = setInterval(function() {
        if (self.autoRefresh && !self.hovering && self.tab === 'live' && !self.streamPaused) {
          self.fetchLogs();
        }
      }, 2000);
    },

    async fetchLogs() {
      if (this.loading) this.loadError = '';
      try {
        var data = await OpenFangAPI.get('/api/audit/recent?n=200');
        this.entries = data.entries || [];
        if (this.autoRefresh && !this.hovering) {
          this.$nextTick(function() {
            var el = document.getElementById('log-container');
            if (el) el.scrollTop = el.scrollHeight;
          });
        }
        if (this.loading) this.loading = false;
      } catch(e) {
        if (this.loading) {
          this.loadError = e.message || 'Could not load logs.';
          this.loading = false;
        }
      }
    },

    async loadData() {
      this.loading = true;
      return this.fetchLogs();
    },

    togglePause: function() {
      this.streamPaused = !this.streamPaused;
      if (!this.streamPaused && this.streamConnected) {
        // Resume: scroll to bottom
        var self = this;
        this.$nextTick(function() {
          var el = document.getElementById('log-container');
          if (el) el.scrollTop = el.scrollHeight;
        });
      }
    },

    clearLogs: function() {
      this.entries = [];
    },

    classifyLevel: function(action) {
      if (!action) return 'info';
      var a = action.toLowerCase();
      if (a.indexOf('error') !== -1 || a.indexOf('fail') !== -1 || a.indexOf('crash') !== -1) return 'error';
      if (a.indexOf('warn') !== -1 || a.indexOf('deny') !== -1 || a.indexOf('block') !== -1) return 'warn';
      return 'info';
    },

    get filteredEntries() {
      var self = this;
      var levelF = this.levelFilter;
      var textF = this.textFilter.toLowerCase();
      return this.entries.filter(function(e) {
        if (levelF && self.classifyLevel(e.action) !== levelF) return false;
        if (textF) {
          var haystack = ((e.action || '') + ' ' + (e.detail || '') + ' ' + (e.agent_id || '')).toLowerCase();
          if (haystack.indexOf(textF) === -1) return false;
        }
        return true;
      });
    },

    get connectionLabel() {
      if (this.streamPaused) return 'Paused';
      if (this.streamConnected) return 'Live';
      if (this._pollTimer) return 'Polling';
      return 'Disconnected';
    },

    get connectionClass() {
      if (this.streamPaused) return 'paused';
      if (this.streamConnected) return 'live';
      if (this._pollTimer) return 'polling';
      return 'disconnected';
    },

    exportLogs: function() {
      var lines = this.filteredEntries.map(function(e) {
        return new Date(e.timestamp).toISOString() + ' [' + e.action + '] ' + (e.detail || '');
      });
      var blob = new Blob([lines.join('\n')], { type: 'text/plain' });
      var url = URL.createObjectURL(blob);
      var a = document.createElement('a');
      a.href = url;
      a.download = 'openfang-logs-' + new Date().toISOString().slice(0, 10) + '.txt';
      a.click();
      URL.revokeObjectURL(url);
    },

    // -- Audit methods --
    get filteredAuditEntries() {
      var self = this;
      if (!self.filterAction) return self.auditEntries;
      return self.auditEntries.filter(function(e) { return e.action === self.filterAction; });
    },

    async loadAudit() {
      this.auditLoading = true;
      this.auditLoadError = '';
      try {
        var data = await OpenFangAPI.get('/api/audit/recent?n=200');
        this.auditEntries = data.entries || [];
        this.tipHash = data.tip_hash || '';
      } catch(e) {
        this.auditEntries = [];
        this.auditLoadError = e.message || 'Could not load audit log.';
      }
      this.auditLoading = false;
    },

    auditAgentName: function(agentId) {
      if (!agentId) return '-';
      var agents = Alpine.store('app').agents || [];
      var agent = agents.find(function(a) { return a.id === agentId; });
      return agent ? agent.name : agentId.substring(0, 8) + '...';
    },

    friendlyAction: function(action) {
      if (!action) return 'Unknown';
      var map = {
        'AgentSpawn': 'Agent Created', 'AgentKill': 'Agent Stopped', 'AgentTerminated': 'Agent Stopped',
        'ToolInvoke': 'Tool Used', 'ToolResult': 'Tool Completed', 'AgentMessage': 'Message',
        'NetworkAccess': 'Network Access', 'ShellExec': 'Shell Command', 'FileAccess': 'File Access',
        'MemoryAccess': 'Memory Access', 'AuthAttempt': 'Login Attempt', 'AuthSuccess': 'Login Success',
        'AuthFailure': 'Login Failed', 'CapabilityDenied': 'Permission Denied', 'RateLimited': 'Rate Limited'
      };
      return map[action] || action.replace(/([A-Z])/g, ' $1').trim();
    },

    async verifyChain() {
      try {
        var data = await OpenFangAPI.get('/api/audit/verify');
        this.chainValid = data.valid === true;
        if (this.chainValid) {
          OpenFangToast.success('Audit chain verified — ' + (data.entries || 0) + ' entries valid');
        } else {
          OpenFangToast.error('Audit chain broken!');
        }
      } catch(e) {
        this.chainValid = false;
        OpenFangToast.error('Chain verification failed: ' + e.message);
      }
    },

    // -- Files tab: ANSI strip + parse + highlight --
    stripAnsi: function(s) {
      if (!s) return '';
      return s
        .replace(/\x1b\][^\x07]*\x07/g, '')
        .replace(/\x1b\[[0-?]*[ -/]*[@-~]/g, '')
        .replace(/\x1b\([0-9A-Za-z]/g, '');
    },

    escapeHtml: function(s) {
      return String(s || '')
        .replace(/&/g, '&amp;')
        .replace(/</g, '&lt;')
        .replace(/>/g, '&gt;')
        .replace(/"/g, '&quot;');
    },

    highlightNeedle: function(s, needle) {
      var plain = String(s || '');
      if (!needle) return this.escapeHtml(plain);
      var lowerP = plain.toLowerCase();
      var nl = needle.toLowerCase();
      var parts = [];
      var pos = 0;
      while (pos < plain.length) {
        var fi = lowerP.indexOf(nl, pos);
        if (fi === -1) {
          parts.push(this.escapeHtml(plain.slice(pos)));
          break;
        }
        parts.push(this.escapeHtml(plain.slice(pos, fi)));
        parts.push('<mark class="log-search-mark">' + this.escapeHtml(plain.slice(fi, fi + needle.length)) + '</mark>');
        pos = fi + needle.length;
      }
      return parts.join('');
    },

    parseLogLineAt: function(line, idx) {
      var plain = this.stripAnsi(line);
      var ts = '';
      var level = '';
      var module = '';
      var message = plain;
      var fieldsRaw = '';
      var m = plain.match(
        /^(\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(?:\.\d+)?(?:Z|[+-]\d{2}:\d{2})?)\s+(ERROR|WARN|INFO|DEBUG|TRACE)\s+([^:]+):\s*(.*)$/
      );
      if (m) {
        ts = m[1];
        level = m[2];
        module = m[3].trim();
        message = m[4];
      } else {
        var m2 = plain.match(/^(ERROR|WARN|INFO|DEBUG|TRACE)\s+([^:]+):\s*(.*)$/);
        if (m2) {
          level = m2[1];
          module = m2[2].trim();
          message = m2[3];
        }
      }
      var bm = message.match(/^(.*?)(\s*\{[\s\S]*\})$/);
      if (bm) {
        message = bm[1].trim();
        fieldsRaw = bm[2].trim();
      }
      return {
        id: idx,
        raw: line,
        plain: plain,
        ts: ts,
        level: level,
        module: module,
        message: message,
        fieldsRaw: fieldsRaw,
      };
    },

    get fileLines() {
      var c = this.fileContent || '';
      if (!c) return [];
      var lines = c.split(/\r?\n/);
      var self = this;
      return lines.map(function(l, i) {
        return self.parseLogLineAt(l, i);
      });
    },

    get filteredFileLines() {
      var q = (this.fileSearch || '').toLowerCase();
      var lv = this.fileLevelFilter;
      return this.fileLines.filter(function(row) {
        if (lv && (row.level || '').toUpperCase() !== lv) return false;
        if (!q) return true;
        var hay = (
          (row.ts || '') +
          ' ' +
          (row.level || '') +
          ' ' +
          (row.module || '') +
          ' ' +
          (row.message || '') +
          ' ' +
          (row.fieldsRaw || '')
        ).toLowerCase();
        return hay.indexOf(q) !== -1;
      });
    },

    get fileRawHtml() {
      var raw = this.fileContent || '';
      try {
        if (typeof AnsiUp !== 'undefined' && AnsiUp) {
          var au = new AnsiUp();
          return au.ansi_to_html(raw);
        }
      } catch(e) { /* fall through */ }
      return this.escapeHtml(raw).replace(/\n/g, '<br>');
    },

    fileCellHtml: function(row, field) {
      var v = row[field] || '';
      return this.highlightNeedle(v, this.fileSearch);
    },

    logLevelClass: function(level) {
      var L = (level || '').toLowerCase();
      if (L === 'error') return 'log-level-error';
      if (L === 'warn') return 'log-level-warn';
      if (L === 'info') return 'log-level-info';
      if (L === 'debug' || L === 'trace') return 'log-level-debug';
      return 'log-level-plain';
    },

    stopFilePoll: function() {
      if (this._filePollTimer) {
        clearInterval(this._filePollTimer);
        this._filePollTimer = null;
      }
    },

    startFilePoll: function() {
      this.stopFilePoll();
      var self = this;
      if (this.tab !== 'files' || !this.fileFollow) return;
      this._filePollTimer = setInterval(function() {
        if (self.tab === 'files' && self.fileFollow) {
          self.refreshFileChunk(self.fileFollow);
        }
      }, 2000);
    },

    async enterFilesTab() {
      this.tab = 'files';
      await this.loadFileList();
      if (!this.selectedFileKey && this.fileList.length) {
        this.selectedFileKey = this.fileList[0].key;
      }
      await this.refreshFileChunk(true);
      this.startFilePoll();
    },

    async loadFileList() {
      this.fileListLoading = true;
      this.fileLoadError = '';
      try {
        var data = await OpenFangAPI.get('/api/logs/files');
        this.fileList = data.files || [];
      } catch(e) {
        this.fileList = [];
        this.fileLoadError = e.message || 'Failed to list log files';
      }
      this.fileListLoading = false;
    },

    async onFileKeyChange() {
      await this.refreshFileChunk(true);
      this.startFilePoll();
    },

    async refreshFileChunk(scrollTail) {
      if (!this.selectedFileKey) return;
      this.fileRefreshing = true;
      if (!scrollTail) this.fileLoadError = '';
      try {
        var listData = await OpenFangAPI.get('/api/logs/files');
        this.fileList = listData.files || [];
        var f = this.fileList.find(function(x) {
          return x.key === this.selectedFileKey;
        }.bind(this));
        if (!f) {
          this.fileContent = '';
          this.fileRefreshing = false;
          return;
        }
        if (!f.exists) {
          this.fileContent = '';
          this.fileRefreshing = false;
          return;
        }
        var lim = this.fileChunkLimit;
        var off = f.size_bytes > lim ? f.size_bytes - lim : 0;
        var path =
          '/api/logs/files/' +
          encodeURIComponent(this.selectedFileKey) +
          '?offset=' +
          off +
          '&limit=' +
          lim;
        var data = await OpenFangAPI.get(path);
        this.fileContent = data.content || '';
        this.fileUtf8Lossy = data.utf8_lossy === true;
        if (scrollTail) {
          this.$nextTick(function() {
            var el = document.getElementById('log-file-container');
            if (el) el.scrollTop = el.scrollHeight;
          });
        }
      } catch(e) {
        this.fileLoadError = e.message || 'Load failed';
      }
      this.fileRefreshing = false;
    },

    setFileFollow: function(on) {
      this.fileFollow = on;
      this.startFilePoll();
    },

    destroy: function() {
      if (this._eventSource) { this._eventSource.close(); this._eventSource = null; }
      if (this._pollTimer) { clearInterval(this._pollTimer); this._pollTimer = null; }
      this.stopFilePoll();
    },
  };
}
