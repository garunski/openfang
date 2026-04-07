// Backlog decisions (project detail) — merged into projectsPage via backlogDecisionsMixins()
'use strict';

function backlogDecisionsMixins() {
  return {
    detailDecisions: [],
    decisionsSelected: null,
    decisionsDetailLoading: false,
    decisionsDetailError: '',
    decisionEditorModalOpen: false,
    decisionEditorMode: 'create',
    decisionEditTargetId: null,
    decisionEditorForm: {
      title: '',
      context: '',
      decision: '',
      consequences: '',
      alternatives: '',
      status: 'proposed',
    },
    decisionEditorSaving: false,
    decisionEditorError: '',

    resetDecisionsCache() {
      this.detailDecisions = [];
      this.decisionsSelected = null;
      this.decisionsDetailLoading = false;
      this.decisionsDetailError = '';
      this.decisionEditorModalOpen = false;
      this.decisionEditorError = '';
    },

    decisionStatusClass(st) {
      if (!st) return 'badge-dim';
      var x = String(st).toLowerCase();
      if (x === 'accepted') return 'badge-success';
      if (x === 'rejected') return 'badge-error';
      if (x === 'superseded') return 'badge-warn';
      return 'badge-info';
    },

    async loadDecisionsTab(force, silent) {
      if (!this.selectedProject) return;
      if (!force && this._detailLoaded.decisions) return;
      var pid = this.selectedProject.id;
      var hideSpinner = !!silent;
      var keepDecisionId =
        hideSpinner && this.decisionsSelected && this.decisionsSelected.id
          ? this.decisionsSelected.id
          : null;
      if (!hideSpinner) {
        this.setDetailLoading('decisions', true);
        this.setDetailError('decisions', '');
        this.decisionsSelected = null;
        this.decisionsDetailError = '';
      }
      try {
        this.detailDecisions = await OpenFangAPI.get(
          '/api/projects/' + encodeURIComponent(pid) + '/backlog/decisions'
        );
        if (!Array.isArray(this.detailDecisions)) this.detailDecisions = [];
        this.setDetailLoaded('decisions', true);
        if (keepDecisionId) await this.decisionsSelectRow({ id: keepDecisionId }, true);
      } catch (e) {
        if (!hideSpinner) {
          this.setDetailError('decisions', e.message || 'Failed to load decisions');
          this.detailDecisions = [];
        }
      }
      if (!hideSpinner) this.setDetailLoading('decisions', false);
    },

    async decisionsSelectRow(row, quietBody) {
      if (!this.selectedProject || !row || !row.id) return;
      var pid = this.selectedProject.id;
      var quiet = !!quietBody;
      if (!quiet) {
        this.decisionsDetailLoading = true;
        this.decisionsDetailError = '';
      }
      try {
        this.decisionsSelected = await OpenFangAPI.get(
          '/api/projects/' + encodeURIComponent(pid) + '/backlog/decisions/' + encodeURIComponent(row.id)
        );
        if (typeof this.pushProjectsHash === 'function') this.pushProjectsHash();
      } catch (e) {
        if (!quiet) {
          this.decisionsSelected = null;
          this.decisionsDetailError = e.message || 'Failed to load decision';
        }
      }
      if (!quiet) this.decisionsDetailLoading = false;
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

    confirmDeleteDecision() {
      var self = this;
      var d = this.decisionsSelected;
      if (!d || !d.id || !this.selectedProject) return;
      var title = (d.title || d.id || '').toString();
      function doit() {
        var pid = self.selectedProject.id;
        var decisionId = d.id;
        OpenFangAPI.del(
          '/api/projects/' +
            encodeURIComponent(pid) +
            '/backlog/decisions/' +
            encodeURIComponent(decisionId)
        )
          .then(function () {
            self.decisionsSelected = null;
            self.decisionsDetailError = '';
            self.setDetailLoaded('decisions', false);
            self.setDetailLoaded('overview', false);
            return self.loadDecisionsTab(true);
          })
          .then(function () {
            if (typeof self.pushProjectsHash === 'function') self.pushProjectsHash();
            if (typeof OpenFangToast !== 'undefined') OpenFangToast.success('Decision deleted');
          })
          .catch(function (e) {
            if (typeof OpenFangToast !== 'undefined') OpenFangToast.error(e.message || 'Delete failed');
          });
      }
      if (typeof OpenFangToast !== 'undefined' && OpenFangToast.confirm) {
        OpenFangToast.confirm(
          'Delete decision',
          'Delete "' + title + '"? This removes the file from disk and cannot be undone.',
          doit
        );
      } else if (window.confirm('Delete "' + title + '"? This cannot be undone.')) {
        doit();
      }
    },

    openDecisionCreateModal() {
      this.decisionEditorMode = 'create';
      this.decisionEditTargetId = null;
      this.decisionEditorForm = {
        title: '',
        context: '',
        decision: '',
        consequences: '',
        alternatives: '',
        status: 'proposed',
      };
      this.decisionEditorError = '';
      this.decisionEditorModalOpen = true;
    },

    openDecisionEditModal() {
      var d = this.decisionsSelected;
      if (!d || !d.id) return;
      var st = d.status != null ? String(d.status).toLowerCase() : 'proposed';
      this.decisionEditorMode = 'edit';
      this.decisionEditTargetId = d.id;
      this.decisionEditorForm = {
        title: d.title != null ? String(d.title) : '',
        context: d.context != null ? String(d.context) : '',
        decision: d.decision != null ? String(d.decision) : '',
        consequences: d.consequences != null ? String(d.consequences) : '',
        alternatives: d.alternatives != null ? String(d.alternatives) : '',
        status: st === 'accepted' || st === 'rejected' || st === 'superseded' ? st : 'proposed',
      };
      this.decisionEditorError = '';
      this.decisionEditorModalOpen = true;
    },

    closeDecisionEditorModal() {
      this.decisionEditorModalOpen = false;
      this.decisionEditorError = '';
      this.decisionEditTargetId = null;
    },

    async submitDecisionEditor() {
      if (!this.selectedProject) return;
      var pid = this.selectedProject.id;
      var base = '/api/projects/' + encodeURIComponent(pid) + '/backlog/decisions';
      this.decisionEditorSaving = true;
      this.decisionEditorError = '';
      try {
        if (this.decisionEditorMode === 'create') {
          var title = (this.decisionEditorForm.title || '').trim();
          if (!title) {
            this.decisionEditorError = 'Title is required';
            this.decisionEditorSaving = false;
            return;
          }
          var created = await OpenFangAPI.post(base, { title: title });
          this.decisionEditorModalOpen = false;
          if (typeof this.setDetailLoaded === 'function') {
            this.setDetailLoaded('decisions', false);
            this.setDetailLoaded('overview', false);
          }
          await this.loadDecisionsTab(true);
          if (created && created.id) await this.decisionsSelectRow({ id: created.id });
          if (typeof this.pushProjectsHash === 'function') this.pushProjectsHash();
          if (typeof OpenFangToast !== 'undefined') OpenFangToast.success('Decision created');
        } else {
          var id = this.decisionEditTargetId;
          if (!id) {
            this.decisionEditorSaving = false;
            return;
          }
          var t = (this.decisionEditorForm.title || '').trim();
          if (!t) {
            this.decisionEditorError = 'Title is required';
            this.decisionEditorSaving = false;
            return;
          }
          var alt = this.decisionEditorForm.alternatives;
          var payload = {
            title: t,
            context: this.decisionEditorForm.context != null ? String(this.decisionEditorForm.context) : '',
            decision: this.decisionEditorForm.decision != null ? String(this.decisionEditorForm.decision) : '',
            consequences:
              this.decisionEditorForm.consequences != null ? String(this.decisionEditorForm.consequences) : '',
            status: this.decisionEditorForm.status || 'proposed',
          };
          if (alt != null && String(alt).trim() !== '') {
            payload.alternatives = String(alt);
          } else {
            payload.alternatives = '';
          }
          var updated = await OpenFangAPI.put(base + '/' + encodeURIComponent(id), payload);
          this.decisionEditorModalOpen = false;
          this.decisionsSelected = updated;
          if (typeof this.setDetailLoaded === 'function') {
            this.setDetailLoaded('decisions', false);
            this.setDetailLoaded('overview', false);
          }
          await this.loadDecisionsTab(true, true);
          if (updated && updated.id) await this.decisionsSelectRow({ id: updated.id }, true);
          if (typeof this.pushProjectsHash === 'function') this.pushProjectsHash();
          if (typeof OpenFangToast !== 'undefined') OpenFangToast.success('Decision saved');
        }
      } catch (e) {
        this.decisionEditorError = e.message || 'Save failed';
      }
      this.decisionEditorSaving = false;
    },
  };
}
