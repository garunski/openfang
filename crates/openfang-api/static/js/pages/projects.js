// Projects page — list, register, project detail (backlog, spokes, agents, pipelines)
'use strict';

function projectsPage() {
  return {
    projects: [],
    projectsLoading: false,
    projectsError: '',
    selectedProject: null,
    detailTab: 'backlog',
    detailTasks: [],
    detailSpokes: [],
    detailAgents: [],
    detailPipelines: [],
    detailLoading: { backlog: false, spokes: false, agents: false, pipelines: false },
    detailErrors: { backlog: '', spokes: '', agents: '', pipelines: '' },
    _detailLoaded: { backlog: false, spokes: false, agents: false, pipelines: false },
    registerModalOpen: false,
    registerForm: { name: '', path: '' },
    registerSubmitting: false,
    registerError: '',
    taskModalOpen: false,
    taskModalTitle: '',
    taskModalLoading: false,
    taskModalError: '',
    taskDetailHtml: '',

    init() {
      this.loadProjects();
    },

    setDetailLoaded(tab, done) {
      var d = Object.assign({}, this._detailLoaded);
      d[tab] = done;
      this._detailLoaded = d;
    },

    resetDetailCache() {
      this._detailLoaded = { backlog: false, spokes: false, agents: false, pipelines: false };
      this.detailTasks = [];
      this.detailSpokes = [];
      this.detailAgents = [];
      this.detailPipelines = [];
      this.detailErrors = { backlog: '', spokes: '', agents: '', pipelines: '' };
      this.detailLoading = { backlog: false, spokes: false, agents: false, pipelines: false };
    },

    setDetailLoading(tab, v) {
      var u = {};
      u[tab] = v;
      this.detailLoading = Object.assign({}, this.detailLoading, u);
    },

    setDetailError(tab, msg) {
      var u = {};
      u[tab] = msg;
      this.detailErrors = Object.assign({}, this.detailErrors, u);
    },

    async loadProjects() {
      this.projectsLoading = true;
      this.projectsError = '';
      try {
        this.projects = await OpenFangAPI.get('/api/projects');
      } catch (e) {
        this.projectsError = e.message || 'Failed to load projects';
        this.projects = [];
      }
      this.projectsLoading = false;
    },

    selectProject(project) {
      this.selectedProject = project;
      this.detailTab = 'backlog';
      this.taskModalOpen = false;
      this.taskDetailHtml = '';
      this.resetDetailCache();
      this.loadDetailTab('backlog');
    },

    backToList() {
      this.selectedProject = null;
      this.taskModalOpen = false;
      this.taskDetailHtml = '';
    },

    retryLoad(tab) {
      this.setDetailLoaded(tab, false);
      this.loadDetailTab(tab, true);
    },

    async loadDetailTab(tab, force) {
      if (!this.selectedProject) return;
      if (!force && this._detailLoaded[tab]) return;
      var pid = this.selectedProject.id;
      this.setDetailLoading(tab, true);
      this.setDetailError(tab, '');
      try {
        if (tab === 'backlog') {
          this.detailTasks = await OpenFangAPI.get('/api/projects/' + encodeURIComponent(pid) + '/tasks');
        } else if (tab === 'spokes') {
          this.detailSpokes = await OpenFangAPI.get('/api/projects/' + encodeURIComponent(pid) + '/spokes');
        } else if (tab === 'agents') {
          this.detailAgents = await OpenFangAPI.get('/api/projects/' + encodeURIComponent(pid) + '/agents');
        } else if (tab === 'pipelines') {
          this.detailPipelines = await OpenFangAPI.get(
            '/api/projects/' + encodeURIComponent(pid) + '/pipelines?limit=50'
          );
        }
        this.setDetailLoaded(tab, true);
      } catch (e) {
        this.setDetailError(tab, e.message || 'Load failed');
      }
      this.setDetailLoading(tab, false);
    },

    async onDetailTabChange(tab) {
      this.detailTab = tab;
      await this.loadDetailTab(tab);
    },

    async discoverSpokes() {
      if (!this.selectedProject) return;
      try {
        await OpenFangAPI.post('/api/projects/' + encodeURIComponent(this.selectedProject.id) + '/discover', {});
        this.setDetailLoaded('spokes', false);
        await this.loadDetailTab('spokes', true);
      } catch (e) {
        this.setDetailError('spokes', e.message || 'Discover failed');
      }
    },

    formatTaskDetailHtml(d) {
      if (!d) return '';
      var h = '';
      if (d.frontmatter && typeof d.frontmatter === 'object' && Object.keys(d.frontmatter).length) {
        h += '<h4 class="text-sm font-semibold mt-0">Frontmatter</h4>';
        h += '<pre class="text-xs overflow-auto max-h-40 p-2 rounded bg-surface-2">' +
          escapeHtml(JSON.stringify(d.frontmatter, null, 2)) +
          '</pre>';
      }
      if (d.description) {
        h += '<h4 class="text-sm font-semibold">Description</h4>';
        h += '<div class="message-bubble markdown-body">' + renderMarkdown(d.description) + '</div>';
      }
      if (d.acceptance_criteria && d.acceptance_criteria.length) {
        h += '<h4 class="text-sm font-semibold">Acceptance criteria</h4><ul class="text-sm">';
        for (var i = 0; i < d.acceptance_criteria.length; i++) {
          var ac = d.acceptance_criteria[i];
          h += '<li>' + escapeHtml(ac.text || '') + (ac.checked ? ' <span class="badge badge-success">done</span>' : '') + '</li>';
        }
        h += '</ul>';
      }
      return h || '<p class="text-dim text-sm">No description.</p>';
    },

    async openTaskDetail(task) {
      if (!this.selectedProject || !task || !task.id) return;
      this.taskModalOpen = true;
      this.taskModalTitle = task.title || task.id;
      this.taskModalLoading = true;
      this.taskModalError = '';
      this.taskDetailHtml = '';
      try {
        var d = await OpenFangAPI.get(
          '/api/projects/' +
            encodeURIComponent(this.selectedProject.id) +
            '/tasks/' +
            encodeURIComponent(task.id)
        );
        this.taskDetailHtml = this.formatTaskDetailHtml(d);
      } catch (e) {
        this.taskModalError = e.message || 'Failed to load task';
      }
      this.taskModalLoading = false;
    },

    closeTaskModal() {
      this.taskModalOpen = false;
      this.taskDetailHtml = '';
      this.taskModalError = '';
    },

    openRegisterModal() {
      this.registerModalOpen = true;
      this.registerError = '';
      this.registerForm = { name: '', path: '' };
    },

    closeRegisterModal() {
      this.registerModalOpen = false;
      this.registerError = '';
    },

    async submitRegister() {
      if (!this.registerForm.name.trim() || !this.registerForm.path.trim()) {
        this.registerError = 'Name and path are required';
        return;
      }
      this.registerSubmitting = true;
      this.registerError = '';
      try {
        await OpenFangAPI.post('/api/projects', {
          name: this.registerForm.name.trim(),
          path: this.registerForm.path.trim(),
        });
        this.closeRegisterModal();
        await this.loadProjects();
      } catch (e) {
        this.registerError = e.message || 'Registration failed';
      }
      this.registerSubmitting = false;
    },

    priorityClass(p) {
      if (!p) return 'badge-dim';
      var x = String(p).toLowerCase();
      if (x === 'high' || x === 'critical') return 'badge-warn';
      if (x === 'low') return 'badge-dim';
      return 'badge-info';
    },

    statusClass(s) {
      if (!s) return 'badge-dim';
      var x = String(s).toLowerCase();
      if (x === 'done' || x === 'closed') return 'badge-success';
      return 'badge-info';
    },

    pipelineOutcomeClass(outcome) {
      if (!outcome) return 'badge-dim';
      var s = String(outcome);
      if (s.indexOf('exit_code=0') !== -1 || s.indexOf('success=true') !== -1) return 'badge-success';
      if (/exit_code=-?[1-9]\d*/.test(s) || s.indexOf('success=false') !== -1) return 'badge-error';
      return 'badge-info';
    },

    truncatePath(path, maxLen) {
      if (!path) return '';
      var n = maxLen || 64;
      if (path.length <= n) return path;
      return '\u2026' + path.slice(-(n - 1));
    },

    async deleteProject(project) {
      if (!project || !project.id) return;
      if (!window.confirm('Delete project "' + (project.name || project.id) + '"?')) return;
      this.projectsError = '';
      try {
        await OpenFangAPI.delete('/api/projects/' + encodeURIComponent(project.id));
        if (this.selectedProject && this.selectedProject.id === project.id) this.backToList();
        await this.loadProjects();
      } catch (e) {
        this.projectsError = e.message || 'Delete failed';
      }
    },
  };
}
