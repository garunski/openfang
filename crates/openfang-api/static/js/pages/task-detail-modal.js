// Backlog task detail modal — merged into projectsPage (board + list)
'use strict';

function backlogTaskDetailMixins() {
  return {
    backlogDetailTask: null,
    taskDetailEditMode: false,
    taskDetailSaving: false,
    taskEditForm: {},
    taskModalTaskActions: true,

    openBacklogTaskDetailFromPayload(task) {
      if (!task || !task.id) return;
      this.taskModalOpen = true;
      this.taskDetailEditMode = false;
      this.taskModalTitle = task.title || task.id;
      this.taskModalLoading = false;
      this.taskModalError = '';
      this.taskModalTaskActions = false;
      this.backlogDetailTask = task;
      this.taskDetailHtml = '';
    },

    async openBacklogTaskDetail(task) {
      if (!this.selectedProject || !task || !task.id) return;
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
    },

    closeTaskModal() {
      this.taskModalOpen = false;
      this.taskDetailHtml = '';
      this.taskModalError = '';
      this.backlogDetailTask = null;
      this.taskDetailEditMode = false;
      this.taskEditForm = {};
      this.taskModalTaskActions = true;
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

    taskDetailEnterEdit() {
      var t = this.backlogDetailTask;
      if (!t) return;
      this.taskEditForm = {
        status: t.status || '',
        priority: (t.priority && String(t.priority)) || '',
        assigneeText: (t.assignee || []).join('\n'),
        labelsText: (t.labels || []).join(', '),
      };
      this.taskDetailEditMode = true;
    },

    taskDetailCancelEdit() {
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
    },

    async taskDetailSaveEdit() {
      if (!this.selectedProject || !this.backlogDetailTask) return;
      this.taskDetailSaving = true;
      this.taskModalError = '';
      try {
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
        var body = {
          status: this.taskEditForm.status || undefined,
          assignee: assignee,
          labels: labels,
        };
        if (this.taskEditForm.priority) body.priority = this.taskEditForm.priority;
        await OpenFangAPI.put(
          '/api/projects/' +
            encodeURIComponent(this.selectedProject.id) +
            '/backlog/tasks/' +
            encodeURIComponent(this.backlogDetailTask.id),
          body
        );
        this.taskDetailEditMode = false;
        this.invalidateBacklogViews();
        await this.backlogRefreshDetailTask();
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
      } catch (e) {
        this.taskModalError = e.message || 'Complete failed';
      }
    },
  };
}
