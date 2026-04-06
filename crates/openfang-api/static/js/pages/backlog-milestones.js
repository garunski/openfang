// Backlog milestones (project detail) — merged into projectsPage via backlogMilestonesMixins()
'use strict';

function backlogMilestonesMixins() {
  return {
    detailMilestones: [],
    detailArchivedMilestones: [],
    milestonesArchivedOpen: false,
    milestoneArchiveBusyId: '',

    resetMilestonesCache() {
      this.detailMilestones = [];
      this.detailArchivedMilestones = [];
      this.milestonesArchivedOpen = false;
      this.milestoneArchiveBusyId = '';
    },

    milestoneDescPreview(m) {
      var d = (m && m.description) || '';
      if (d.length > 200) return d.slice(0, 200) + '\u2026';
      return d;
    },

    milestoneProgressPct(m) {
      if (!m || m.progressPercent == null) return 0;
      var x = Number(m.progressPercent);
      if (Number.isNaN(x)) return 0;
      return Math.max(0, Math.min(100, x));
    },

    async loadMilestonesTab(force) {
      if (!this.selectedProject) return;
      if (!force && this._detailLoaded.milestones) return;
      var pid = this.selectedProject.id;
      this.setDetailLoading('milestones', true);
      this.setDetailError('milestones', '');
      try {
        var base = '/api/projects/' + encodeURIComponent(pid) + '/backlog/milestones';
        var active = await OpenFangAPI.get(base);
        var arch = await OpenFangAPI.get(base + '/archived');
        this.detailMilestones = Array.isArray(active) ? active : [];
        this.detailArchivedMilestones = Array.isArray(arch) ? arch : [];
        this.setDetailLoaded('milestones', true);
      } catch (e) {
        this.setDetailError('milestones', e.message || 'Failed to load milestones');
        this.detailMilestones = [];
        this.detailArchivedMilestones = [];
      }
      this.setDetailLoading('milestones', false);
    },

    async milestoneArchive(m) {
      if (!this.selectedProject || !m || !m.id) return;
      var pid = this.selectedProject.id;
      this.milestoneArchiveBusyId = m.id;
      try {
        await OpenFangAPI.post(
          '/api/projects/' +
            encodeURIComponent(pid) +
            '/backlog/milestones/' +
            encodeURIComponent(m.id) +
            '/archive',
          {}
        );
        this.setDetailLoaded('milestones', false);
        await this.loadMilestonesTab(true);
      } catch (e) {
        this.setDetailError('milestones', e.message || 'Archive failed');
      }
      this.milestoneArchiveBusyId = '';
    },
  };
}
