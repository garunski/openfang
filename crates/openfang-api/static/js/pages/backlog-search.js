// Project-detail backlog search — merged into projectsPage via backlogSearchMixins()
'use strict';

function backlogSearchExcerptTask(t) {
  var body = (t && (t.description || t.rawContent)) || '';
  body = String(body).replace(/\s+/g, ' ').trim();
  if (body.length > 140) return body.slice(0, 140) + '\u2026';
  return body || '\u2014';
}

function backlogSearchExcerptDoc(d) {
  var body = (d && d.rawContent) || '';
  body = String(body).replace(/\s+/g, ' ').trim();
  if (body.length > 140) return body.slice(0, 140) + '\u2026';
  return body || '\u2014';
}

function backlogSearchExcerptDecision(d) {
  if (!d) return '\u2014';
  var body = [d.context, d.decision, d.consequences].join(' ').replace(/\s+/g, ' ').trim();
  if (body.length > 140) return body.slice(0, 140) + '\u2026';
  return body || '\u2014';
}

function backlogSearchMixins() {
  return {
    backlogSearchQuery: '',
    backlogSearchResults: [],
    backlogSearchLoading: false,
    backlogSearchOpen: false,
    _backlogSearchTimer: null,

    resetBacklogSearch() {
      if (this._backlogSearchTimer) {
        clearTimeout(this._backlogSearchTimer);
        this._backlogSearchTimer = null;
      }
      this.backlogSearchQuery = '';
      this.backlogSearchResults = [];
      this.backlogSearchLoading = false;
      this.backlogSearchOpen = false;
    },

    onBacklogSearchInput() {
      var self = this;
      if (this._backlogSearchTimer) clearTimeout(this._backlogSearchTimer);
      this._backlogSearchTimer = setTimeout(function () {
        self.runBacklogSearch();
      }, 300);
    },

    backlogSearchHitTitle(hit) {
      if (!hit) return '';
      if (hit.type === 'task' && hit.task) return hit.task.title || hit.task.id || '';
      if (hit.type === 'document' && hit.document) return hit.document.title || hit.document.id || '';
      if (hit.type === 'decision' && hit.decision) return hit.decision.title || hit.decision.id || '';
      return '';
    },

    backlogSearchHitExcerpt(hit) {
      if (!hit) return '';
      if (hit.type === 'task' && hit.task) return backlogSearchExcerptTask(hit.task);
      if (hit.type === 'document' && hit.document) return backlogSearchExcerptDoc(hit.document);
      if (hit.type === 'decision' && hit.decision) return backlogSearchExcerptDecision(hit.decision);
      return '';
    },

    backlogSearchTypeLabel(hit) {
      if (!hit) return '';
      if (hit.type === 'task') return 'Task';
      if (hit.type === 'document') return 'Doc';
      if (hit.type === 'decision') return 'Decision';
      return String(hit.type || '');
    },

    async runBacklogSearch() {
      if (!this.selectedProject) return;
      var q = (this.backlogSearchQuery || '').trim();
      if (!q) {
        this.backlogSearchResults = [];
        this.backlogSearchLoading = false;
        return;
      }
      this.backlogSearchLoading = true;
      var pid = this.selectedProject.id;
      var url =
        '/api/projects/' +
        encodeURIComponent(pid) +
        '/backlog/search?' +
        new URLSearchParams({ q: q }).toString();
      try {
        var rows = await OpenFangAPI.get(url);
        this.backlogSearchResults = Array.isArray(rows) ? rows : [];
      } catch (e) {
        this.backlogSearchResults = [];
      }
      this.backlogSearchLoading = false;
    },

    async openBacklogSearchHit(hit) {
      this.backlogSearchOpen = false;
      if (!hit || !hit.type) return;
      if (hit.type === 'task' && hit.task) {
        if (typeof this.openBacklogTaskDetailFromPayload === 'function') {
          this.openBacklogTaskDetailFromPayload(hit.task);
        } else {
          this.openBacklogTaskDetail(hit.task);
        }
        return;
      }
      if (hit.type === 'document' && hit.document) {
        this.detailTab = 'docs';
        await this.loadDocsTab(true);
        await this.docsSelectDoc(hit.document.id);
        return;
      }
      if (hit.type === 'decision' && hit.decision) {
        this.detailTab = 'decisions';
        await this.loadDecisionsTab(true);
        await this.decisionsSelectRow(hit.decision, false, { openModal: true });
      }
    },
  };
}
