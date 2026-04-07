// Task list table (project detail List tab) — merged into projectsPage
'use strict';

function backlogListMixins() {
  return {
    listFilterStatus: '',
    listFilterPriority: '',
    listFilterLabels: [],
    listFilterAssignee: '',
    listSortKey: 'id',
    listSortDir: 'desc',
    listConfigStatuses: [],
    listUniqueLabels: [],
    listUniqueAssignees: [],
    backlogCleanupModalOpen: false,
    cleanupAgeInput: '30',
    cleanupPreview: null,
    cleanupPreviewLoading: false,
    cleanupExecuteLoading: false,
    cleanupModalError: '',

    resetListFilters() {
      this.listFilterStatus = '';
      this.listFilterPriority = '';
      this.listFilterLabels = [];
      this.listFilterAssignee = '';
      this.listSortKey = 'id';
      this.listSortDir = 'desc';
      this.listConfigStatuses = [];
      this.listUniqueLabels = [];
      this.listUniqueAssignees = [];
      this.backlogCleanupModalOpen = false;
      this.cleanupAgeInput = '30';
      this.cleanupPreview = null;
      this.cleanupPreviewLoading = false;
      this.cleanupExecuteLoading = false;
      this.cleanupModalError = '';
    },

    openBacklogCleanupModal() {
      this.cleanupModalError = '';
      this.cleanupPreview = null;
      this.backlogCleanupModalOpen = true;
      this.refreshBacklogCleanupPreview();
    },

    closeBacklogCleanupModal() {
      this.backlogCleanupModalOpen = false;
      this.cleanupPreview = null;
      this.cleanupModalError = '';
      this.cleanupPreviewLoading = false;
    },

    async refreshBacklogCleanupPreview() {
      if (!this.selectedProject) return;
      var age = parseInt(String(this.cleanupAgeInput || '0').trim(), 10);
      if (Number.isNaN(age) || age < 0) {
        this.cleanupModalError = 'Enter a valid age in days (0 or more).';
        this.cleanupPreview = null;
        return;
      }
      this.cleanupPreviewLoading = true;
      this.cleanupModalError = '';
      try {
        var pid = this.selectedProject.id;
        this.cleanupPreview = await OpenFangAPI.get(
          '/api/projects/' +
            encodeURIComponent(pid) +
            '/backlog/tasks/cleanup?age=' +
            encodeURIComponent(String(age))
        );
      } catch (e) {
        this.cleanupModalError = e.message || 'Preview failed';
        this.cleanupPreview = null;
      }
      this.cleanupPreviewLoading = false;
    },

    executeBacklogCleanup() {
      var self = this;
      if (!this.selectedProject) return;
      var n = this.cleanupPreview && this.cleanupPreview.count != null ? Number(this.cleanupPreview.count) : 0;
      if (n === 0) {
        this._executeBacklogCleanupConfirmed();
        return;
      }
      var msg =
        'Move ' +
        n +
        ' Done task(s) from backlog/tasks to backlog/completed/? Same behavior as Backlog.md cleanup.';
      function doit() {
        self._executeBacklogCleanupConfirmed();
      }
      if (typeof OpenFangToast !== 'undefined' && OpenFangToast.confirm) {
        OpenFangToast.confirm('Cleanup old tasks', msg, doit);
      } else if (window.confirm(msg)) {
        doit();
      }
    },

    async _executeBacklogCleanupConfirmed() {
      if (!this.selectedProject) return;
      var age = parseInt(String(this.cleanupAgeInput || '0').trim(), 10);
      if (Number.isNaN(age) || age < 0) {
        this.cleanupModalError = 'Invalid age';
        return;
      }
      this.cleanupExecuteLoading = true;
      this.cleanupModalError = '';
      try {
        var pid = this.selectedProject.id;
        var r = await OpenFangAPI.post(
          '/api/projects/' + encodeURIComponent(pid) + '/backlog/tasks/cleanup/execute',
          { age: age }
        );
        this.closeBacklogCleanupModal();
        this.setDetailLoaded('backlog', false);
        this.setDetailLoaded('milestones', false);
        this.setDetailLoaded('overview', false);
        if (typeof this.resetBoardCache === 'function') this.resetBoardCache();
        if (typeof this.resetPertCache === 'function') this.resetPertCache();
        await this.loadDetailTab('backlog', true);
        if (typeof OpenFangToast !== 'undefined') {
          var mc = r && r.movedCount != null ? Number(r.movedCount) : 0;
          if (mc > 0) OpenFangToast.success('Moved ' + mc + ' task(s) to completed');
          else OpenFangToast.info('No tasks were moved');
        }
      } catch (e) {
        this.cleanupModalError = e.message || 'Cleanup failed';
      }
      this.cleanupExecuteLoading = false;
    },

    rebuildListFilterOptions() {
      var tasks = this.detailTasks || [];
      var labels = {};
      var assignees = {};
      for (var i = 0; i < tasks.length; i++) {
        var t = tasks[i];
        var j;
        if (t.labels) {
          for (j = 0; j < t.labels.length; j++) labels[t.labels[j]] = true;
        }
        if (t.assignee) {
          for (j = 0; j < t.assignee.length; j++) assignees[t.assignee[j]] = true;
        }
      }
      this.listUniqueLabels = Object.keys(labels).sort();
      this.listUniqueAssignees = Object.keys(assignees).sort();
    },

    listToggleSort(key) {
      if (this.listSortKey === key) {
        this.listSortDir = this.listSortDir === 'asc' ? 'desc' : 'asc';
      } else {
        this.listSortKey = key;
        this.listSortDir = key === 'id' ? 'desc' : 'asc';
      }
    },

    /** Numeric suffix from ids like TASK-36; null if no trailing digits. */
    listTaskIdNumericSuffix(id) {
      var m = String(id || '').match(/-(\d+)$/);
      return m ? parseInt(m[1], 10) : null;
    },

    listSortIndicator(key) {
      if (this.listSortKey !== key) return '';
      return this.listSortDir === 'asc' ? ' \u25B2' : ' \u25BC';
    },

    listDepsCount(t) {
      return Array.isArray(t.dependencies) ? t.dependencies.length : 0;
    },

    listAssigneeStr(t) {
      if (!t.assignee || !t.assignee.length) return '\u2014';
      return t.assignee.join(', ');
    },

    listCreatedStr(t) {
      return t.createdDate || t.created_date || '\u2014';
    },

    listLabelsStr(t) {
      if (!t.labels || !t.labels.length) return '\u2014';
      return t.labels.join(', ');
    },

    listTaskMatchesFilters(t) {
      if (this.listFilterStatus) {
        if (
          String(t.status || '').toLowerCase() !== String(this.listFilterStatus).toLowerCase()
        ) {
          return false;
        }
      }
      if (this.listFilterPriority) {
        var p = (t.priority && String(t.priority).toLowerCase()) || '';
        if (p !== String(this.listFilterPriority).toLowerCase()) return false;
      }
      if (this.listFilterAssignee && String(this.listFilterAssignee).trim()) {
        var want = String(this.listFilterAssignee).trim().toLowerCase();
        var ok = false;
        var ax = t.assignee || [];
        for (var i = 0; i < ax.length; i++) {
          if (String(ax[i]).toLowerCase().indexOf(want) !== -1) {
            ok = true;
            break;
          }
        }
        if (!ok) return false;
      }
      if (this.listFilterLabels && this.listFilterLabels.length) {
        var tls = t.labels || [];
        var selected = this.listFilterLabels;
        var any = false;
        for (var s = 0; s < selected.length; s++) {
          var lab = selected[s];
          for (var j = 0; j < tls.length; j++) {
            if (tls[j] === lab) {
              any = true;
              break;
            }
          }
        }
        if (!any) return false;
      }
      return true;
    },

    listSortValue(t, key) {
      switch (key) {
        case 'id':
          return String(t.id || '');
        case 'title':
          return String(t.title || '').toLowerCase();
        case 'status':
          return String(t.status || '').toLowerCase();
        case 'priority':
          return String((t.priority && String(t.priority)) || '').toLowerCase();
        case 'labels':
          return this.listLabelsStr(t).toLowerCase();
        case 'assignee':
          return this.listAssigneeStr(t).toLowerCase();
        case 'created':
          return this.listCreatedStr(t);
        case 'deps':
          return this.listDepsCount(t);
        default:
          return '';
      }
    },

    listSortCompare(a, b) {
      var inv = this.listSortDir === 'desc' ? -1 : 1;
      if (this.listSortKey === 'id') {
        var na = this.listTaskIdNumericSuffix(a.id);
        var nb = this.listTaskIdNumericSuffix(b.id);
        var hasA = na != null;
        var hasB = nb != null;
        if (hasA && hasB && na !== nb) {
          return this.listSortDir === 'desc' ? nb - na : na - nb;
        }
        if (hasA !== hasB) {
          return hasA ? -1 : 1;
        }
        return String(a.id || '').localeCompare(String(b.id || '')) * inv;
      }
      var ka = this.listSortValue(a, this.listSortKey);
      var kb = this.listSortValue(b, this.listSortKey);
      if (this.listSortKey === 'deps') {
        if (ka < kb) return -1 * inv;
        if (ka > kb) return 1 * inv;
        return String(a.id || '').localeCompare(String(b.id || '')) * inv;
      }
      if (ka < kb) return -1 * inv;
      if (ka > kb) return 1 * inv;
      return String(a.id || '').localeCompare(String(b.id || '')) * inv;
    },

    listStatusFilterOptions() {
      var seen = {};
      var out = [];
      function add(s) {
        if (s == null || s === '') return;
        var k = String(s).toLowerCase();
        if (seen[k]) return;
        seen[k] = true;
        out.push(s);
      }
      var i;
      for (i = 0; i < (this.listConfigStatuses || []).length; i++) add(this.listConfigStatuses[i]);
      var tasks = this.detailTasks || [];
      for (i = 0; i < tasks.length; i++) add(tasks[i].status);
      return out;
    },

    listPriorityFilterOptions() {
      return ['high', 'medium', 'low'];
    },

    listTableRows() {
      var tasks = Array.isArray(this.detailTasks) ? this.detailTasks.slice() : [];
      var out = [];
      var i;
      for (i = 0; i < tasks.length; i++) {
        if (this.listTaskMatchesFilters(tasks[i])) out.push(tasks[i]);
      }
      var self = this;
      out.sort(function (a, b) {
        return self.listSortCompare(a, b);
      });
      return out;
    },
  };
}
