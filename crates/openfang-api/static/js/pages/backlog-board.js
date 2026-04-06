// Kanban board (project detail) — merged into projectsPage via backlogBoardMixins()
'use strict';

function backlogBoardMixins() {
  return {
    boardStatuses: [],
    boardTasks: [],
    boardLoading: false,
    boardError: '',
    _boardLoaded: false,
    boardNewTaskOpen: false,
    boardNewTask: { title: '', status: '', priority: '' },
    boardNewTaskSaving: false,
    boardNewTaskError: '',
    boardDragId: null,
    boardDragStatus: null,
    _boardClickGuard: 0,

    boardCardClick(task) {
      if (this._boardClickGuard && Date.now() < this._boardClickGuard) return;
      this.openBacklogTaskDetail(task);
    },

    resetBoardCache() {
      this._boardLoaded = false;
      this.boardTasks = [];
      this.boardStatuses = [];
      this.boardError = '';
      this.boardLoading = false;
    },

    async loadBoardData(force) {
      if (!this.selectedProject) return;
      if (!force && this._boardLoaded) return;
      var pid = this.selectedProject.id;
      this.boardLoading = true;
      this.boardError = '';
      try {
        var cfg = await OpenFangAPI.get(
          '/api/projects/' + encodeURIComponent(pid) + '/backlog/config'
        );
        var tasks = await OpenFangAPI.get(
          '/api/projects/' + encodeURIComponent(pid) + '/backlog/tasks'
        );
        this.boardStatuses = this.normalizeBoardStatuses(cfg, tasks);
        this.boardTasks = Array.isArray(tasks) ? tasks : [];
        this.ensureBoardColumnsCoverTasks();
        this._boardLoaded = true;
      } catch (e) {
        this.boardError = e.message || 'Failed to load board';
      }
      this.boardLoading = false;
    },

    ensureBoardColumnsCoverTasks() {
      var cols = this.boardStatuses.slice();
      var lower = {};
      var i;
      var c;
      for (c = 0; c < cols.length; c++) {
        lower[String(cols[c]).toLowerCase()] = true;
      }
      for (i = 0; i < this.boardTasks.length; i++) {
        var st = this.boardTasks[i].status;
        if (!st) continue;
        var k = String(st).toLowerCase();
        if (!lower[k]) {
          lower[k] = true;
          cols.push(st);
        }
      }
      this.boardStatuses = cols;
    },

    normalizeBoardStatuses(cfg, tasks) {
      var s = cfg && cfg.statuses && cfg.statuses.length ? cfg.statuses.slice() : [];
      if (s.length) return s;
      var seen = {};
      var out = [];
      if (tasks) {
        for (var i = 0; i < tasks.length; i++) {
          var st = tasks[i].status;
          if (st && !seen[st]) {
            seen[st] = true;
            out.push(st);
          }
        }
      }
      return out.length ? out : ['Open'];
    },

    boardStatusKey(taskStatus, columnStatus) {
      return (
        String(taskStatus || '').trim().toLowerCase() ===
        String(columnStatus || '').trim().toLowerCase()
      );
    },

    boardTasksForStatus(statusName) {
      var sn = String(statusName);
      var self = this;
      return this.boardTasks
        .filter(function (t) {
          return self.boardStatusKey(t.status, sn);
        })
        .sort(function (a, b) {
          return self.compareBoardTasks(a, b);
        });
    },

    compareBoardTasks(a, b) {
      var oa = a.ordinal != null ? Number(a.ordinal) : 1e9;
      var ob = b.ordinal != null ? Number(b.ordinal) : 1e9;
      if (oa !== ob) return oa - ob;
      var da = a.createdDate || a.created_date || '';
      var db = b.createdDate || b.created_date || '';
      if (da < db) return -1;
      if (da > db) return 1;
      return String(a.id || '').localeCompare(String(b.id || ''));
    },

    boardColumnCount(statusName) {
      return this.boardTasksForStatus(statusName).length;
    },

    boardPriorityClass(p) {
      if (!p) return 'kanban-prio-none';
      var x = String(p).toLowerCase();
      if (x === 'high') return 'kanban-prio-high';
      if (x === 'medium') return 'kanban-prio-medium';
      if (x === 'low') return 'kanban-prio-low';
      return 'kanban-prio-none';
    },

    boardAssigneeInitials(name) {
      if (!name) return '?';
      var parts = String(name).trim().split(/\s+/);
      if (parts.length >= 2) {
        return (parts[0][0] + parts[1][0]).toUpperCase();
      }
      return String(name).slice(0, 2).toUpperCase();
    },

    boardOpenNewTask() {
      this.boardNewTaskError = '';
      var def = (this.boardStatuses && this.boardStatuses[0]) || 'Open';
      this.boardNewTask = { title: '', status: def, priority: '' };
      this.boardNewTaskOpen = true;
    },

    boardCloseNewTask() {
      this.boardNewTaskOpen = false;
    },

    async boardSubmitNewTask() {
      if (!this.selectedProject || !this.boardNewTask.title.trim()) {
        this.boardNewTaskError = 'Title required';
        return;
      }
      this.boardNewTaskSaving = true;
      this.boardNewTaskError = '';
      var body = {
        title: this.boardNewTask.title.trim(),
        status: this.boardNewTask.status || undefined,
      };
      if (this.boardNewTask.priority) body.priority = this.boardNewTask.priority;
      try {
        await OpenFangAPI.post(
          '/api/projects/' + encodeURIComponent(this.selectedProject.id) + '/backlog/tasks',
          body
        );
        this.boardCloseNewTask();
        this._boardLoaded = false;
        this.setDetailLoaded('backlog', false);
        await this.loadBoardData(true);
      } catch (e) {
        this.boardNewTaskError = e.message || 'Create failed';
      }
      this.boardNewTaskSaving = false;
    },

    boardDragStart(task, columnStatus, ev) {
      this.boardDragId = task.id;
      this.boardDragStatus = columnStatus;
      try {
        ev.dataTransfer.effectAllowed = 'move';
        ev.dataTransfer.setData('text/plain', task.id);
      } catch (err) {}
    },

    boardDragEnd() {
      this._boardClickGuard = Date.now() + 250;
      this.boardDragId = null;
      this.boardDragStatus = null;
    },

    boardAllowDrop(ev) {
      ev.preventDefault();
    },

    async boardDropOnColumn(targetStatus, ev) {
      ev.preventDefault();
      var id = this.boardDragId;
      var from = this.boardDragStatus;
      if (!id || !this.selectedProject) {
        this.boardDragEnd();
        return;
      }
      var ts = targetStatus;
      var same = String(from || '').toLowerCase() === String(ts || '').toLowerCase();
      var colTasks = this.boardTasksForStatus(ts);
      var orderedIds = colTasks.map(function (t) {
        return t.id;
      });
      var fi = orderedIds.indexOf(id);
      if (fi >= 0) orderedIds.splice(fi, 1);
      orderedIds.push(id);
      await this.boardPersistOrder(id, ts, orderedIds, same);
      this.boardDragEnd();
    },

    async boardDropOnCard(targetStatus, beforeTaskId, ev) {
      ev.preventDefault();
      ev.stopPropagation();
      var id = this.boardDragId;
      var from = this.boardDragStatus;
      if (!id || !this.selectedProject) {
        this.boardDragEnd();
        return;
      }
      var colTasks = this.boardTasksForStatus(targetStatus);
      var orderedIds = colTasks.map(function (t) {
        return t.id;
      });
      var fi = orderedIds.indexOf(id);
      if (fi >= 0) orderedIds.splice(fi, 1);
      var bi = orderedIds.indexOf(beforeTaskId);
      if (bi < 0) orderedIds.push(id);
      else orderedIds.splice(bi, 0, id);
      var same =
        String(from || '').toLowerCase() === String(targetStatus || '').toLowerCase();
      await this.boardPersistOrder(id, targetStatus, orderedIds, same);
      this.boardDragEnd();
    },

    async boardPersistOrder(taskId, targetStatus, orderedIds, sameColumn) {
      var pid = this.selectedProject.id;
      this.boardError = '';
      try {
        if (!sameColumn) {
          await OpenFangAPI.put(
            '/api/projects/' + encodeURIComponent(pid) + '/backlog/tasks/' + encodeURIComponent(taskId),
            { status: targetStatus }
          );
        }
        await OpenFangAPI.post(
          '/api/projects/' + encodeURIComponent(pid) + '/backlog/tasks/reorder',
          {
            taskId: taskId,
            targetStatus: targetStatus,
            orderedTaskIds: orderedIds,
          }
        );
      } catch (e) {
        this.boardError = e.message || 'Update failed';
      }
      this._boardLoaded = false;
      this.setDetailLoaded('backlog', false);
      await this.loadBoardData(true);
    },
  };
}
