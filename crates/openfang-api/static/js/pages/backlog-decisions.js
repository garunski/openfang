// Backlog decisions (project detail) — merged into projectsPage via backlogDecisionsMixins()
'use strict';

function backlogDecisionsMixins() {
  return {
    detailDecisions: [],
    decisionsSelected: null,
    decisionsDetailLoading: false,
    decisionsDetailError: '',

    resetDecisionsCache() {
      this.detailDecisions = [];
      this.decisionsSelected = null;
      this.decisionsDetailLoading = false;
      this.decisionsDetailError = '';
    },

    decisionStatusClass(st) {
      if (!st) return 'badge-dim';
      var x = String(st).toLowerCase();
      if (x === 'accepted') return 'badge-success';
      if (x === 'rejected') return 'badge-error';
      if (x === 'superseded') return 'badge-warn';
      return 'badge-info';
    },

    async loadDecisionsTab(force) {
      if (!this.selectedProject) return;
      if (!force && this._detailLoaded.decisions) return;
      var pid = this.selectedProject.id;
      this.setDetailLoading('decisions', true);
      this.setDetailError('decisions', '');
      this.decisionsSelected = null;
      this.decisionsDetailError = '';
      try {
        this.detailDecisions = await OpenFangAPI.get(
          '/api/projects/' + encodeURIComponent(pid) + '/backlog/decisions'
        );
        if (!Array.isArray(this.detailDecisions)) this.detailDecisions = [];
        this.setDetailLoaded('decisions', true);
      } catch (e) {
        this.setDetailError('decisions', e.message || 'Failed to load decisions');
        this.detailDecisions = [];
      }
      this.setDetailLoading('decisions', false);
    },

    async decisionsSelectRow(row) {
      if (!this.selectedProject || !row || !row.id) return;
      var pid = this.selectedProject.id;
      this.decisionsDetailLoading = true;
      this.decisionsDetailError = '';
      try {
        this.decisionsSelected = await OpenFangAPI.get(
          '/api/projects/' + encodeURIComponent(pid) + '/backlog/decisions/' + encodeURIComponent(row.id)
        );
      } catch (e) {
        this.decisionsSelected = null;
        this.decisionsDetailError = e.message || 'Failed to load decision';
      }
      this.decisionsDetailLoading = false;
    },

    decisionSectionHtml(field) {
      var d = this.decisionsSelected;
      if (!d) return '';
      var text = d[field];
      if (text == null || text === '') return '<p class="text-dim text-sm">\u2014</p>';
      return typeof renderMarkdown === 'function' ? renderMarkdown(String(text)) : '';
    },

    decisionsRowActive(row) {
      return this.decisionsSelected && row && this.decisionsSelected.id === row.id;
    },
  };
}
