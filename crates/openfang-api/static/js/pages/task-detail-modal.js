// Backlog task detail modal — merged into projectsPage (board + list)
'use strict';

/** Milestone id from API/task payload (camelCase or rare aliases). */
function taskMilestoneFieldFromPayload(t) {
  if (!t) return '';
  var v =
    t.milestone !== undefined && t.milestone !== null
      ? t.milestone
      : t.milestoneId !== undefined && t.milestoneId !== null
        ? t.milestoneId
        : '';
  return String(v).trim();
}

function backlogTaskDetailMixins() {
  return {
    backlogDetailTask: null,
    taskDetailEditMode: false,
    taskDetailSaving: false,
    taskEditForm: {},
    /** Bumps when entering edit so milestone `<select>` remounts (Alpine + dynamic options). */
    taskEditSessionKey: 0,
    taskModalTaskActions: true,
    _taskEasymde: null,
    /** Raw JSON from GET /api/conduits (retained for debugging / future use). */
    taskConduitsList: [],
    /**
     * Filtered rows for the `<select>` (plain array — not a getter) so Alpine re-renders options
     * after async fetch inside nested `x-if`; getters here did not reliably refresh `x-for`.
     */
    taskConduitsOptions: [],
    taskConduitsLoading: false,
    taskConduitSelectedId: '',
    taskConduitRunning: false,
    taskConduitRunError: '',

    syncTaskConduitsOptions() {
      var list = this.taskConduitsList || [];
      this.taskConduitsOptions = list.filter(function (w) {
        return w && w.id != null && String(w.id).trim() !== '';
      });
    },

    taskConduitOptionLabel(w) {
      var steps = w.steps != null ? w.steps : '?';
      return (w.name || w.id) + ' (' + steps + ' steps)';
    },

    taskDetailDestroyEasymde() {
      var m = this._taskEasymde;
      if (!m) return;
      ['desc', 'plan', 'notes', 'summary'].forEach(function (k) {
        var inst = m[k];
        if (inst) {
          try {
            inst.toTextArea();
          } catch (e1) {}
        }
      });
      this._taskEasymde = null;
    },

    taskDetailSyncEasymdeToForm() {
      var m = this._taskEasymde;
      if (!m || !this.taskEditForm) return;
      if (m.desc) this.taskEditForm.description = m.desc.value();
      if (m.plan) this.taskEditForm.implementationPlan = m.plan.value();
      if (m.notes) this.taskEditForm.implementationNotes = m.notes.value();
      if (m.summary) this.taskEditForm.finalSummary = m.summary.value();
    },

    taskDetailMountEasymde() {
      var self = this;
      this.taskDetailDestroyEasymde();
      if (typeof EasyMDE === 'undefined' || !this.taskDetailEditMode) return;
      this._taskEasymde = {};

      function textareaFor(refKey, dataAttr) {
        var el = self.$refs[refKey];
        if (el && el.tagName === 'TEXTAREA') return el;
        var modal = document.querySelector('.of-modal--task-detail');
        if (!modal || !modal.isConnected) return null;
        var byData = modal.querySelector('textarea[data-task-easymde="' + dataAttr + '"]');
        return byData && byData.tagName === 'TEXTAREA' ? byData : null;
      }

      function mount(refKey, dataAttr, storeKey, minH, initial) {
        var el = textareaFor(refKey, dataAttr);
        if (!el) return;
        el.value = initial != null ? String(initial) : '';
        try {
          var inst = new EasyMDE(Object.assign({ element: el }, openfangEasymdeBaseOpts(minH)));
          self._taskEasymde[storeKey] = inst;
        } catch (e) {
          console.warn('[OpenFang] EasyMDE failed for', dataAttr, e);
        }
      }

      function runMounts() {
        if (!self.taskDetailEditMode || typeof EasyMDE === 'undefined') return;
        mount('taskEasymdeDesc', 'desc', 'desc', '220px', self.taskEditForm.description);
        mount('taskEasymdePlan', 'plan', 'plan', '160px', self.taskEditForm.implementationPlan);
        mount('taskEasymdeNotes', 'notes', 'notes', '160px', self.taskEditForm.implementationNotes);
        mount('taskEasymdeSummary', 'summary', 'summary', '120px', self.taskEditForm.finalSummary);
      }

      function afterStableLayout(fn) {
        requestAnimationFrame(function () {
          requestAnimationFrame(fn);
        });
      }

      if (typeof this.$nextTick === 'function') {
        this.$nextTick(function () {
          afterStableLayout(runMounts);
        });
      } else {
        queueMicrotask(function () {
          afterStableLayout(runMounts);
        });
      }
    },

    async loadTaskWorkflowsForModal() {
      this.taskConduitRunError = '';
      if (!this.selectedProject) {
        this.taskConduitsList = [];
        this.syncTaskConduitsOptions();
        return;
      }
      this.taskConduitsLoading = true;
      try {
        var data = await OpenFangAPI.get('/api/conduits');
        this.taskConduitsList = Array.isArray(data) ? data : [];
        this.syncTaskConduitsOptions();
        var rows = this.taskConduitsOptions;
        if (
          this.taskConduitSelectedId &&
          !rows.some(function (w) {
            return String(w.id) === String(this.taskConduitSelectedId);
          }, this)
        ) {
          this.taskConduitSelectedId = rows.length ? String(rows[0].id) : '';
        } else if (!this.taskConduitSelectedId && rows.length) {
          this.taskConduitSelectedId = String(rows[0].id);
        }
      } catch (e) {
        this.taskConduitsList = [];
        this.syncTaskConduitsOptions();
        this.taskConduitRunError = e.message || 'Could not load workflows';
      }
      this.taskConduitsLoading = false;
    },

    async startTaskWorkflowFromModal() {
      if (!this.selectedProject || !this.backlogDetailTask || !this.taskConduitSelectedId) return;
      this.taskConduitRunning = true;
      this.taskConduitRunError = '';
      try {
        var body = { conduitId: this.taskConduitSelectedId, postMattermostConfirmation: false };
        var res = await OpenFangAPI.post(
          '/api/projects/' +
            encodeURIComponent(this.selectedProject.id) +
            '/backlog/tasks/' +
            encodeURIComponent(this.backlogDetailTask.id) +
            '/conduit-start',
          body
        );
        if (typeof OpenFangToast !== 'undefined' && OpenFangToast.success) {
          var rid = res && res.run_id ? res.run_id : '';
          OpenFangToast.success(
            rid ? 'Workflow started (run ' + rid.slice(0, 8) + '…)' : 'Workflow started'
          );
        }
        this.closeTaskModal();
        if (typeof this.invalidateBacklogViews === 'function') this.invalidateBacklogViews();
        if (this.detailTab === 'overview' && typeof this.loadDetailTab === 'function') {
          await this.loadDetailTab('overview', true);
        }
      } catch (e) {
        this.taskConduitRunError = e.message || 'Workflow start failed';
        if (typeof OpenFangToast !== 'undefined' && OpenFangToast.error) {
          OpenFangToast.error(this.taskConduitRunError);
        }
      }
      this.taskConduitRunning = false;
    },

    openBacklogTaskDetailFromPayload(task) {
      if (!task || !task.id) return;
      this.taskDetailDestroyEasymde();
      this.taskModalOpen = true;
      this.taskDetailEditMode = false;
      this.taskModalTitle = task.title || task.id;
      this.taskModalLoading = false;
      this.taskModalError = '';
      this.taskModalTaskActions = false;
      this.backlogDetailTask = task;
      this.taskDetailHtml = '';
      this.taskConduitSelectedId = '';
      var self = this;
      if (typeof this.loadTaskWorkflowsForModal === 'function') {
        this.loadTaskWorkflowsForModal().catch(function () {});
      }
    },

    async openBacklogTaskDetail(task) {
      if (!this.selectedProject || !task || !task.id) return;
      this.taskDetailDestroyEasymde();
      this.taskModalOpen = true;
      this.taskDetailEditMode = false;
      this.taskModalTaskActions = true;
      this.taskModalTitle = task.title || task.id;
      this.taskModalLoading = true;
      this.taskModalError = '';
      this.backlogDetailTask = null;
      this.taskDetailHtml = '';
      try {
        this.backlogDetailTask = await OpenFangAPI.get(
          '/api/projects/' +
            encodeURIComponent(this.selectedProject.id) +
            '/backlog/tasks/' +
            encodeURIComponent(task.id)
        );
        this.taskModalTitle = this.backlogDetailTask.title || this.backlogDetailTask.id;
      } catch (e) {
        this.taskModalError = e.message || 'Failed to load task';
      }
      this.taskModalLoading = false;
      this.taskConduitSelectedId = '';
      if (!this.taskModalError && typeof this.loadTaskWorkflowsForModal === 'function') {
        this.loadTaskWorkflowsForModal().catch(function () {});
      }
    },

    closeTaskModal() {
      this.taskDetailDestroyEasymde();
      this.taskModalOpen = false;
      this.taskDetailHtml = '';
      this.taskModalError = '';
      this.backlogDetailTask = null;
      this.taskDetailEditMode = false;
      this.taskEditForm = {};
      this.taskModalTaskActions = true;
      this.taskConduitsList = [];
      this.taskConduitsOptions = [];
      this.taskConduitSelectedId = '';
      this.taskConduitRunError = '';
      this.taskConduitRunning = false;
    },

    backlogDetailMd(text) {
      if (!text) return '';
      return typeof renderMarkdown === 'function' ? renderMarkdown(text) : escapeHtml(text);
    },

    taskJoin(arr) {
      if (!arr || !arr.length) return '\u2014';
      return arr.join(', ');
    },

    taskEditStatusOptions() {
      var s = this.listConfigStatuses;
      if (s && s.length) return s;
      return ['Open', 'In Progress', 'Done'];
    },

    async taskDetailEnterEdit() {
      var t = this.backlogDetailTask;
      if (!t) return;
      if (this.selectedProject && typeof this.loadMilestonesTab === 'function') {
        try {
          await this.loadMilestonesTab(false);
        } catch (e1) {}
      }
      this.taskDetailDestroyEasymde();
      this.taskEditSessionKey = (this.taskEditSessionKey || 0) + 1;
      var savedMilestone = taskMilestoneFieldFromPayload(t);
      var acSrc = t.acceptanceCriteriaItems || t.acceptance_criteria || [];
      var acEditRows = [];
      var i;
      for (i = 0; i < acSrc.length; i++) {
        acEditRows.push({
          checked: !!acSrc[i].checked,
          text: acSrc[i].text != null ? String(acSrc[i].text) : '',
        });
      }
      if (acEditRows.length === 0) {
        acEditRows.push({ checked: false, text: '' });
      }
      this.taskEditForm = {
        title: t.title || '',
        status: t.status || '',
        priority: (t.priority && String(t.priority).toLowerCase()) || '',
        assigneeText: (t.assignee || []).join('\n'),
        labelsText: (t.labels || []).join(', '),
        description: t.description != null ? String(t.description) : '',
        implementationPlan:
          t.implementationPlan != null
            ? String(t.implementationPlan)
            : t.implementation_plan != null
              ? String(t.implementation_plan)
              : '',
        implementationNotes:
          t.implementationNotes != null
            ? String(t.implementationNotes)
            : t.implementation_notes != null
              ? String(t.implementation_notes)
              : '',
        finalSummary:
          t.finalSummary != null
            ? String(t.finalSummary)
            : t.final_summary != null
              ? String(t.final_summary)
              : '',
        acEditRows: acEditRows,
        milestoneId: savedMilestone,
      };
      this.taskDetailEditMode = true;
      var self = this;
      if (typeof this.$nextTick === 'function') {
        this.$nextTick(function () {
          if (self.taskEditForm && savedMilestone) {
            self.taskEditForm.milestoneId = savedMilestone;
          }
        });
      }
      this.taskDetailMountEasymde();
    },

    taskDetailAddAcRow() {
      if (!this.taskEditForm.acEditRows) this.taskEditForm.acEditRows = [];
      this.taskEditForm.acEditRows.push({ checked: false, text: '' });
    },

    taskDetailRemoveAcRow(idx) {
      var rows = this.taskEditForm.acEditRows;
      if (!rows || rows.length <= 1) return;
      rows.splice(idx, 1);
    },

    taskAcDoneCount() {
      var a = this.backlogDetailTask && this.backlogDetailTask.acceptanceCriteriaItems;
      if (!a || !a.length) return 0;
      var n = 0;
      var i;
      for (i = 0; i < a.length; i++) {
        if (a[i].checked) n++;
      }
      return n;
    },

    taskAcTotalCount() {
      var a = this.backlogDetailTask && this.backlogDetailTask.acceptanceCriteriaItems;
      return a && a.length ? a.length : 0;
    },

    taskDetailPriorityDisplay() {
      var t = this.backlogDetailTask;
      if (!t || !t.priority) return '\u2014';
      var p = t.priority;
      if (typeof p === 'string') return p;
      return String(p);
    },

    /** Options for milestone `<select>` including (none), orphan current id, and active milestones. */
    taskDetailMilestoneSelectRows() {
      var out = [{ value: '', text: '(none)' }];
      var cur = String((this.taskEditForm && this.taskEditForm.milestoneId) || '').trim();
      var list = this.detailMilestones || [];
      var i;
      var inList = false;
      for (i = 0; i < list.length; i++) {
        var mid = String(list[i].id != null ? list[i].id : '').trim();
        if (mid && mid === cur) inList = true;
      }
      if (cur && !inList) {
        out.push({
          value: cur,
          text: 'Current: ' + cur + ' (not in active list)',
        });
      }
      for (i = 0; i < list.length; i++) {
        var m = list[i];
        var id = String(m.id != null ? m.id : '').trim();
        out.push({
          value: id,
          text: (m.title || id || m.id) + ' \u2014 ' + id,
        });
      }
      return out;
    },

    taskDetailCancelEdit() {
      this.taskDetailDestroyEasymde();
      this.taskDetailEditMode = false;
    },

    async backlogRefreshDetailTask() {
      if (!this.selectedProject || !this.backlogDetailTask || !this.backlogDetailTask.id) return;
      var id = this.backlogDetailTask.id;
      this.backlogDetailTask = await OpenFangAPI.get(
        '/api/projects/' +
          encodeURIComponent(this.selectedProject.id) +
          '/backlog/tasks/' +
          encodeURIComponent(id)
      );
      this.taskModalTitle = this.backlogDetailTask.title || this.backlogDetailTask.id;
    },

    invalidateBacklogViews() {
      this.setDetailLoaded('backlog', false);
      if (typeof this.resetBoardCache === 'function') this.resetBoardCache();
      if (typeof this.resetPertCache === 'function') this.resetPertCache();
    },

    async taskDetailSaveEdit() {
      if (!this.selectedProject || !this.backlogDetailTask) return;
      this.taskDetailSaving = true;
      this.taskModalError = '';
      try {
        this.taskDetailSyncEasymdeToForm();
        var labels = String(this.taskEditForm.labelsText || '')
          .split(',')
          .map(function (s) {
            return s.trim();
          })
          .filter(Boolean);
        var assignee = String(this.taskEditForm.assigneeText || '')
          .split('\n')
          .map(function (s) {
            return s.trim();
          })
          .filter(Boolean);
        var rows = this.taskEditForm.acEditRows || [];
        var ac = [];
        for (var j = 0; j < rows.length; j++) {
          var tx = String(rows[j].text || '').trim();
          if (!tx) continue;
          ac.push({
            index: ac.length,
            text: tx,
            checked: !!rows[j].checked,
          });
        }
        var body = {
          status: this.taskEditForm.status || undefined,
          assignee: assignee,
          labels: labels,
          description: String(this.taskEditForm.description || ''),
          implementationPlan: String(this.taskEditForm.implementationPlan || ''),
          implementationNotes: String(this.taskEditForm.implementationNotes || ''),
          finalSummary: String(this.taskEditForm.finalSummary || ''),
          acceptanceCriteriaItems: ac,
        };
        var titleTrim = String(this.taskEditForm.title || '').trim();
        if (titleTrim) body.title = titleTrim;
        if (this.taskEditForm.priority) body.priority = this.taskEditForm.priority;
        body.milestone = String(this.taskEditForm.milestoneId || '').trim();
        await OpenFangAPI.put(
          '/api/projects/' +
            encodeURIComponent(this.selectedProject.id) +
            '/backlog/tasks/' +
            encodeURIComponent(this.backlogDetailTask.id),
          body
        );
        this.taskDetailDestroyEasymde();
        this.taskDetailEditMode = false;
        this.invalidateBacklogViews();
        await this.backlogRefreshDetailTask();
        if (this.detailTab === 'pert' && typeof this.loadPertTab === 'function') await this.loadPertTab(true);
      } catch (e) {
        this.taskModalError = e.message || 'Save failed';
      }
      this.taskDetailSaving = false;
    },

    async toggleBacklogAc(index) {
      if (!this.selectedProject || !this.backlogDetailTask) return;
      this.taskModalError = '';
      try {
        await OpenFangAPI.put(
          '/api/projects/' +
            encodeURIComponent(this.selectedProject.id) +
            '/backlog/tasks/' +
            encodeURIComponent(this.backlogDetailTask.id),
          { toggleAc: index }
        );
        this.invalidateBacklogViews();
        await this.backlogRefreshDetailTask();
      } catch (e) {
        this.taskModalError = e.message || 'Toggle failed';
      }
    },

    async backlogArchiveTask() {
      if (!this.selectedProject || !this.backlogDetailTask) return;
      if (!window.confirm('Archive this task (moves file to archive)?')) return;
      this.taskModalError = '';
      try {
        await OpenFangAPI.delete(
          '/api/projects/' +
            encodeURIComponent(this.selectedProject.id) +
            '/backlog/tasks/' +
            encodeURIComponent(this.backlogDetailTask.id)
        );
        this.invalidateBacklogViews();
        this.closeTaskModal();
        if (this.detailTab === 'backlog') await this.loadDetailTab('backlog', true);
        if (this.detailTab === 'board') await this.loadBoardData(true);
        if (this.detailTab === 'pert' && typeof this.loadPertTab === 'function') await this.loadPertTab(true);
      } catch (e) {
        this.taskModalError = e.message || 'Archive failed';
      }
    },

    async backlogCompleteTask() {
      if (!this.selectedProject || !this.backlogDetailTask) return;
      if (!window.confirm('Mark task complete (moves to completed)?')) return;
      this.taskModalError = '';
      try {
        await OpenFangAPI.post(
          '/api/projects/' +
            encodeURIComponent(this.selectedProject.id) +
            '/backlog/tasks/' +
            encodeURIComponent(this.backlogDetailTask.id) +
            '/complete',
          {}
        );
        this.invalidateBacklogViews();
        this.closeTaskModal();
        if (this.detailTab === 'backlog') await this.loadDetailTab('backlog', true);
        if (this.detailTab === 'board') await this.loadBoardData(true);
        if (this.detailTab === 'pert' && typeof this.loadPertTab === 'function') await this.loadPertTab(true);
      } catch (e) {
        this.taskModalError = e.message || 'Complete failed';
      }
    },
  };
}
