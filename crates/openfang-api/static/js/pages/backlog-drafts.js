// Backlog drafts (project detail) — merged into projectsPage via backlogDraftsMixins()
'use strict';

function backlogDraftsMixins() {
  return {
    detailDrafts: [],
    draftPromotingId: '',
    draftPromoteError: '',

    resetDraftsCache() {
      this.detailDrafts = [];
      this.draftPromotingId = '';
      this.draftPromoteError = '';
    },

    draftCreatedDisplay(d) {
      if (!d) return '\u2014';
      return d.createdDate || '\u2014';
    },

    async loadDraftsTab(force) {
      if (!this.selectedProject) return;
      if (!force && this._detailLoaded.drafts) return;
      var pid = this.selectedProject.id;
      this.setDetailLoading('drafts', true);
      this.setDetailError('drafts', '');
      this.draftPromoteError = '';
      try {
        this.detailDrafts = await OpenFangAPI.get(
          '/api/projects/' + encodeURIComponent(pid) + '/backlog/drafts'
        );
        if (!Array.isArray(this.detailDrafts)) this.detailDrafts = [];
        this.setDetailLoaded('drafts', true);
      } catch (e) {
        this.setDetailError('drafts', e.message || 'Failed to load drafts');
        this.detailDrafts = [];
      }
      this.setDetailLoading('drafts', false);
    },

    async draftPromoteToTask(d) {
      if (!this.selectedProject || !d || !d.id) return;
      var pid = this.selectedProject.id;
      this.draftPromotingId = d.id;
      this.draftPromoteError = '';
      try {
        await OpenFangAPI.post(
          '/api/projects/' + encodeURIComponent(pid) + '/backlog/drafts/' + encodeURIComponent(d.id) + '/promote',
          {}
        );
        this.setDetailLoaded('drafts', false);
        this.setDetailLoaded('backlog', false);
        if (typeof this.resetBoardCache === 'function') this.resetBoardCache();
        await this.loadDraftsTab(true);
        if (this.detailTab === 'backlog') await this.loadDetailTab('backlog', true);
        if (this.detailTab === 'board') await this.loadBoardData(true);
      } catch (e) {
        this.draftPromoteError = e.message || 'Promote failed';
      }
      this.draftPromotingId = '';
    },
  };
}
