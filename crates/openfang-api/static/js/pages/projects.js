// Projects page — list, register, project detail (backlog, spokes, agents, pipelines)
'use strict';

function projectsPage() {
  return Object.assign(
    {
    projects: [],
    projectsLoading: false,
    projectsError: '',
    selectedProject: null,
    detailTab: 'backlog',
    detailTasks: [],
    detailSpokes: [],
    detailAgents: [],
    detailPipelines: [],
    detailLoading: {
      backlog: false,
      spokes: false,
      agents: false,
      pipelines: false,
      docs: false,
      decisions: false,
      drafts: false,
      milestones: false,
    },
    detailErrors: {
      backlog: '',
      spokes: '',
      agents: '',
      pipelines: '',
      docs: '',
      decisions: '',
      drafts: '',
      milestones: '',
    },
    _detailLoaded: {
      backlog: false,
      spokes: false,
      agents: false,
      pipelines: false,
      docs: false,
      decisions: false,
      drafts: false,
      milestones: false,
    },
    registerModalOpen: false,
    registerForm: { name: '', path: '' },
    registerSubmitting: false,
    registerError: '',
    bindAgentForm: { agent_id: '' },
    bindAgentSubmitting: false,
    bindAgentError: '',
    taskModalOpen: false,
    taskModalTitle: '',
    taskModalLoading: false,
    taskModalError: '',
    taskDetailHtml: '',

    init() {
      this.loadProjects();
      if (typeof OpenFangAPI.backlogFeedEnsureConnected === 'function') {
        OpenFangAPI.backlogFeedEnsureConnected();
      }
      if (!this._backlogLiveBound) {
        this._backlogLiveBound = true;
        var self = this;
        window.addEventListener('openfang-backlog-updated', function (ev) {
          self.onBacklogLiveUpdate(ev.detail);
        });
      }
    },

    async onBacklogLiveUpdate(detail) {
      if (!detail || !this.selectedProject) return;
      if (String(detail.project_id) !== String(this.selectedProject.id)) return;
      var tabs = ['backlog', 'board', 'docs', 'decisions', 'drafts', 'milestones'];
      var i;
      for (i = 0; i < tabs.length; i++) {
        this.setDetailLoaded(tabs[i], false);
      }
      if (typeof this.resetBoardCache === 'function') this.resetBoardCache();
      await this.loadDetailTab(this.detailTab, true);
    },

    setDetailLoaded(tab, done) {
      var d = Object.assign({}, this._detailLoaded);
      d[tab] = done;
      this._detailLoaded = d;
    },

    resetDetailCache() {
      this._detailLoaded = {
        backlog: false,
        spokes: false,
        agents: false,
        pipelines: false,
        docs: false,
        decisions: false,
        drafts: false,
        milestones: false,
      };
      this.detailTasks = [];
      this.detailSpokes = [];
      this.detailAgents = [];
      this.detailPipelines = [];
      this.bindAgentForm = { agent_id: '' };
      this.bindAgentError = '';
      this.detailErrors = {
        backlog: '',
        spokes: '',
        agents: '',
        pipelines: '',
        docs: '',
        decisions: '',
        drafts: '',
        milestones: '',
      };
      this.detailLoading = {
        backlog: false,
        spokes: false,
        agents: false,
        pipelines: false,
        docs: false,
        decisions: false,
        drafts: false,
        milestones: false,
      };
      if (typeof this.resetBoardCache === 'function') this.resetBoardCache();
      if (typeof this.resetListFilters === 'function') this.resetListFilters();
      if (typeof this.resetDocsCache === 'function') this.resetDocsCache();
      if (typeof this.resetDecisionsCache === 'function') this.resetDecisionsCache();
      if (typeof this.resetDraftsCache === 'function') this.resetDraftsCache();
      if (typeof this.resetMilestonesCache === 'function') this.resetMilestonesCache();
      if (typeof this.resetBacklogSearch === 'function') this.resetBacklogSearch();
      this.backlogDetailTask = null;
      this.taskDetailEditMode = false;
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
      this.backlogDetailTask = null;
      this.resetDetailCache();
      this.loadDetailTab('backlog');
    },

    backToList() {
      this.selectedProject = null;
      this.taskModalOpen = false;
      this.taskDetailHtml = '';
      this.backlogDetailTask = null;
      if (typeof this.resetBacklogSearch === 'function') this.resetBacklogSearch();
    },

    retryLoad(tab) {
      this.setDetailLoaded(tab, false);
      this.loadDetailTab(tab, true);
    },

    async loadDetailTab(tab, force) {
      if (!this.selectedProject) return;
      if (tab === 'board') {
        await this.loadBoardData(!!force);
        return;
      }
      if (tab === 'docs') {
        await this.loadDocsTab(!!force);
        return;
      }
      if (tab === 'decisions') {
        await this.loadDecisionsTab(!!force);
        return;
      }
      if (tab === 'drafts') {
        await this.loadDraftsTab(!!force);
        return;
      }
      if (tab === 'milestones') {
        await this.loadMilestonesTab(!!force);
        return;
      }
      if (!force && this._detailLoaded[tab]) return;
      var pid = this.selectedProject.id;
      this.setDetailLoading(tab, true);
      this.setDetailError(tab, '');
      try {
        if (tab === 'backlog') {
          var cfg = await OpenFangAPI.get(
            '/api/projects/' + encodeURIComponent(pid) + '/backlog/config'
          );
          this.listConfigStatuses =
            cfg && cfg.statuses && cfg.statuses.length ? cfg.statuses.slice() : [];
          this.detailTasks = await OpenFangAPI.get(
            '/api/projects/' + encodeURIComponent(pid) + '/backlog/tasks'
          );
          if (typeof this.rebuildListFilterOptions === 'function') this.rebuildListFilterOptions();
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

    async bindProjectAgent() {
      if (!this.selectedProject) return;
      var aid = (this.bindAgentForm.agent_id || '').trim();
      if (!aid) {
        this.bindAgentError = 'Agent id is required';
        return;
      }
      this.bindAgentSubmitting = true;
      this.bindAgentError = '';
      try {
        await OpenFangAPI.post(
          '/api/projects/' + encodeURIComponent(this.selectedProject.id) + '/agents',
          { agent_id: aid }
        );
        OpenFangToast.success('Agent bound to project');
        this.bindAgentForm.agent_id = '';
        this.setDetailLoaded('agents', false);
        await this.loadDetailTab('agents', true);
      } catch (e) {
        this.bindAgentError = e.message || 'Bind failed';
      }
      this.bindAgentSubmitting = false;
    },

    async unbindProjectAgent(agent) {
      if (!this.selectedProject || !agent || !agent.agent_id) return;
      if (agent.binding !== 'explicit') return;
      this.bindAgentSubmitting = true;
      this.bindAgentError = '';
      try {
        await OpenFangAPI.del(
          '/api/projects/' +
            encodeURIComponent(this.selectedProject.id) +
            '/agents/' +
            encodeURIComponent(agent.agent_id)
        );
        OpenFangToast.success('Binding removed');
        this.setDetailLoaded('agents', false);
        await this.loadDetailTab('agents', true);
      } catch (e) {
        this.bindAgentError = e.message || 'Unbind failed';
      }
      this.bindAgentSubmitting = false;
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
  },
    typeof backlogBoardMixins === 'function' ? backlogBoardMixins() : {},
    typeof backlogListMixins === 'function' ? backlogListMixins() : {},
    typeof backlogTaskDetailMixins === 'function' ? backlogTaskDetailMixins() : {},
    typeof backlogDocsMixins === 'function' ? backlogDocsMixins() : {},
    typeof backlogDecisionsMixins === 'function' ? backlogDecisionsMixins() : {},
    typeof backlogDraftsMixins === 'function' ? backlogDraftsMixins() : {},
    typeof backlogMilestonesMixins === 'function' ? backlogMilestonesMixins() : {},
    typeof backlogSearchMixins === 'function' ? backlogSearchMixins() : {}
  );
}

