// Backlog milestones (project detail) — merged into projectsPage via backlogMilestonesMixins()
'use strict';

function backlogMilestonesMixins() {
  return {
    detailMilestones: [],
    detailArchivedMilestones: [],
    milestonesArchivedOpen: false,
    milestoneArchiveBusyId: '',
    milestoneDeleteBusyId: '',
    milestoneEditorModalOpen: false,
    milestoneEditorMode: 'create',
    milestoneEditTargetId: null,
    milestoneEditorForm: { title: '', description: '' },
    milestoneEditorSaving: false,
    milestoneEditorError: '',
    milestoneAddTaskModalOpen: false,
    milestoneAddTaskMilestone: null,
    milestoneAddTaskRows: [],
    milestoneAddTaskTaskId: '',
    milestoneAddTaskLoading: false,
    milestoneAddTaskSaving: false,
    milestoneAddTaskError: '',

    resetMilestonesCache() {
      this.detailMilestones = [];
      this.detailArchivedMilestones = [];
      this.milestonesArchivedOpen = false;
      this.milestoneArchiveBusyId = '';
      this.milestoneDeleteBusyId = '';
      this.milestoneEditorModalOpen = false;
      this.milestoneEditorError = '';
      this.milestoneAddTaskModalOpen = false;
      this.milestoneAddTaskMilestone = null;
      this.milestoneAddTaskRows = [];
      this.milestoneAddTaskTaskId = '';
      this.milestoneAddTaskError = '';
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

    async loadMilestonesTab(force, silent) {
      if (!this.selectedProject) return;
      if (!force && this._detailLoaded.milestones) return;
      var pid = this.selectedProject.id;
      var hideSpinner = !!silent;
      if (!hideSpinner) {
        this.setDetailLoading('milestones', true);
        this.setDetailError('milestones', '');
      }
      try {
        var base = '/api/projects/' + encodeURIComponent(pid) + '/backlog/milestones';
        var active = await OpenFangAPI.get(base);
        var arch = await OpenFangAPI.get(base + '/archived');
        this.detailMilestones = Array.isArray(active) ? active : [];
        this.detailArchivedMilestones = Array.isArray(arch) ? arch : [];
        this.setDetailLoaded('milestones', true);
      } catch (e) {
        if (!hideSpinner) {
          this.setDetailError('milestones', e.message || 'Failed to load milestones');
          this.detailMilestones = [];
          this.detailArchivedMilestones = [];
        }
      }
      if (!hideSpinner) this.setDetailLoading('milestones', false);
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
        this.setDetailLoaded('overview', false);
        if (typeof this.resetPertCache === 'function') this.resetPertCache();
        await this.loadMilestonesTab(true);
      } catch (e) {
        this.setDetailError('milestones', e.message || 'Archive failed');
      }
      this.milestoneArchiveBusyId = '';
      this.milestoneDeleteBusyId = '';
    },

    openMilestoneCreateModal() {
      this.milestoneEditorMode = 'create';
      this.milestoneEditTargetId = null;
      this.milestoneEditorForm = { title: '', description: '' };
      this.milestoneEditorError = '';
      this.milestoneEditorModalOpen = true;
    },

    openMilestoneEditModal(m) {
      if (!m || !m.id) return;
      this.milestoneEditorMode = 'edit';
      this.milestoneEditTargetId = m.id;
      this.milestoneEditorForm = {
        title: m.title != null ? String(m.title) : '',
        description: m.description != null ? String(m.description) : '',
      };
      this.milestoneEditorError = '';
      this.milestoneEditorModalOpen = true;
    },

    closeMilestoneEditorModal() {
      this.milestoneEditorModalOpen = false;
      this.milestoneEditorError = '';
      this.milestoneEditTargetId = null;
    },

    closeMilestoneAddTaskModal() {
      this.milestoneAddTaskModalOpen = false;
      this.milestoneAddTaskMilestone = null;
      this.milestoneAddTaskRows = [];
      this.milestoneAddTaskTaskId = '';
      this.milestoneAddTaskError = '';
      this.milestoneAddTaskLoading = false;
    },

    async openMilestoneAddTaskModal(m) {
      if (!this.selectedProject || !m || !m.id) return;
      this.milestoneAddTaskMilestone = m;
      this.milestoneAddTaskTaskId = '';
      this.milestoneAddTaskError = '';
      this.milestoneAddTaskRows = [];
      this.milestoneAddTaskModalOpen = true;
      this.milestoneAddTaskLoading = true;
      try {
        var pid = this.selectedProject.id;
        var tasks = await OpenFangAPI.get(
          '/api/projects/' + encodeURIComponent(pid) + '/backlog/tasks'
        );
        if (!Array.isArray(tasks)) tasks = [];
        tasks.sort(function (a, b) {
          return String(a.id).localeCompare(String(b.id), undefined, { numeric: true, sensitivity: 'base' });
        });
        this.milestoneAddTaskRows = tasks.map(function (t) {
          var ms =
            t.milestone != null && String(t.milestone).trim() !== ''
              ? String(t.milestone).trim()
              : '';
          var cur = ms ? ' \u2014 current milestone: ' + ms : '';
          return {
            id: t.id,
            label: t.id + ' \u2014 ' + (t.title || '(no title)') + cur,
          };
        });
      } catch (e) {
        this.milestoneAddTaskError = e.message || 'Failed to load tasks';
      }
      this.milestoneAddTaskLoading = false;
    },

    async submitMilestoneAddTask() {
      if (!this.selectedProject || !this.milestoneAddTaskMilestone || !this.milestoneAddTaskMilestone.id) return;
      var tid = String(this.milestoneAddTaskTaskId || '').trim();
      if (!tid) {
        this.milestoneAddTaskError = 'Select a task';
        return;
      }
      var mid = this.milestoneAddTaskMilestone.id;
      this.milestoneAddTaskSaving = true;
      this.milestoneAddTaskError = '';
      try {
        await OpenFangAPI.put(
          '/api/projects/' +
            encodeURIComponent(this.selectedProject.id) +
            '/backlog/tasks/' +
            encodeURIComponent(tid),
          { milestone: mid }
        );
        this.closeMilestoneAddTaskModal();
        this.setDetailLoaded('milestones', false);
        this.setDetailLoaded('overview', false);
        this.setDetailLoaded('backlog', false);
        if (typeof this.resetBoardCache === 'function') this.resetBoardCache();
        if (typeof this.resetPertCache === 'function') this.resetPertCache();
        await this.loadMilestonesTab(true, true);
        if (typeof this.pushProjectsHash === 'function') this.pushProjectsHash();
        if (typeof OpenFangToast !== 'undefined') OpenFangToast.success('Task assigned to milestone');
      } catch (e) {
        this.milestoneAddTaskError = e.message || 'Update failed';
      }
      this.milestoneAddTaskSaving = false;
    },

    confirmDeleteMilestone(m) {
      var self = this;
      if (!m || !m.id || !this.selectedProject) return;
      var title = (m.title || m.id || '').toString();
      function doit() {
        self.milestoneDeleteBusyId = m.id;
        var pid = self.selectedProject.id;
        OpenFangAPI.del(
          '/api/projects/' +
            encodeURIComponent(pid) +
            '/backlog/milestones/' +
            encodeURIComponent(m.id)
        )
          .then(function () {
            self.setDetailLoaded('milestones', false);
            self.setDetailLoaded('overview', false);
            return self.loadMilestonesTab(true);
          })
          .then(function () {
            if (typeof self.pushProjectsHash === 'function') self.pushProjectsHash();
            if (typeof OpenFangToast !== 'undefined') OpenFangToast.success('Milestone deleted');
          })
          .catch(function (e) {
            if (typeof OpenFangToast !== 'undefined') OpenFangToast.error(e.message || 'Delete failed');
          })
          .finally(function () {
            self.milestoneDeleteBusyId = '';
          });
      }
      if (typeof OpenFangToast !== 'undefined' && OpenFangToast.confirm) {
        OpenFangToast.confirm(
          'Delete milestone',
          'Delete "' + title + '"? This removes the file from disk and cannot be undone.',
          doit
        );
      } else if (window.confirm('Delete "' + title + '"? This cannot be undone.')) {
        doit();
      }
    },

    async submitMilestoneEditor() {
      if (!this.selectedProject) return;
      var pid = this.selectedProject.id;
      var base = '/api/projects/' + encodeURIComponent(pid) + '/backlog/milestones';
      this.milestoneEditorSaving = true;
      this.milestoneEditorError = '';
      try {
        if (this.milestoneEditorMode === 'create') {
          var title = (this.milestoneEditorForm.title || '').trim();
          if (!title) {
            this.milestoneEditorError = 'Title is required';
            this.milestoneEditorSaving = false;
            return;
          }
          var desc = this.milestoneEditorForm.description != null ? String(this.milestoneEditorForm.description) : '';
          await OpenFangAPI.post(base, { title: title, description: desc });
          this.milestoneEditorModalOpen = false;
          this.setDetailLoaded('milestones', false);
          this.setDetailLoaded('overview', false);
          await this.loadMilestonesTab(true);
          if (typeof this.pushProjectsHash === 'function') this.pushProjectsHash();
          if (typeof OpenFangToast !== 'undefined') OpenFangToast.success('Milestone created');
        } else {
          var id = this.milestoneEditTargetId;
          if (!id) {
            this.milestoneEditorSaving = false;
            return;
          }
          var t = (this.milestoneEditorForm.title || '').trim();
          if (!t) {
            this.milestoneEditorError = 'Title is required';
            this.milestoneEditorSaving = false;
            return;
          }
          var d = this.milestoneEditorForm.description != null ? String(this.milestoneEditorForm.description) : '';
          await OpenFangAPI.put(base + '/' + encodeURIComponent(id), { title: t, description: d });
          this.milestoneEditorModalOpen = false;
          this.setDetailLoaded('milestones', false);
          this.setDetailLoaded('overview', false);
          await this.loadMilestonesTab(true);
          if (typeof this.pushProjectsHash === 'function') this.pushProjectsHash();
          if (typeof OpenFangToast !== 'undefined') OpenFangToast.success('Milestone saved');
        }
      } catch (e) {
        this.milestoneEditorError = e.message || 'Save failed';
      }
      this.milestoneEditorSaving = false;
    },
  };
}
