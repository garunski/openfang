// Projects page — list, register, project detail (backlog, spokes, agents, pipelines)
'use strict';

var PROJECT_DETAIL_TAB_SET = {
  overview: true,
  backlog: true,
  board: true,
  pert: true,
  docs: true,
  decisions: true,
  milestones: true,
  spokes: true,
  agents: true,
  pipelines: true,
  workflows: true,
};

function projectsPage() {
  return Object.assign(
    {
    projects: [],
    projectsLoading: false,
    projectsError: '',
    selectedProject: null,
    detailTab: 'overview',
    projectOverview: null,
    detailTasks: [],
    detailSpokes: [],
    detailAgents: [],
    detailPipelines: [],
    projectWorkflows: [],
    wfRunModal: null,
    wfRunInput: '',
    wfRunSubmitting: false,
    wfRunError: '',
    detailLoading: {
      overview: false,
      backlog: false,
      pert: false,
      spokes: false,
      agents: false,
      pipelines: false,
      workflows: false,
      docs: false,
      decisions: false,
      milestones: false,
    },
    detailErrors: {
      overview: '',
      backlog: '',
      pert: '',
      spokes: '',
      agents: '',
      pipelines: '',
      workflows: '',
      docs: '',
      decisions: '',
      milestones: '',
    },
    _detailLoaded: {
      overview: false,
      backlog: false,
      pert: false,
      spokes: false,
      agents: false,
      pipelines: false,
      workflows: false,
      docs: false,
      decisions: false,
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
    /** Min ms between WS-driven backlog refreshes (matches server watch debounce; avoids refetch storms). */
    _backlogWsReloadNextAt: 0,

    init() {
      var self = this;
      this.loadProjects();
      if (typeof OpenFangAPI.backlogFeedEnsureConnected === 'function') {
        OpenFangAPI.backlogFeedEnsureConnected();
      }
      if (!this._backlogLiveBound) {
        this._backlogLiveBound = true;
        window.addEventListener('openfang-backlog-updated', function (ev) {
          self.onBacklogLiveUpdate(ev.detail);
        });
      }
      window.addEventListener('hashchange', function () {
        self.syncProjectsFromHash();
      });
    },

    isValidProjectDetailTab(t) {
      return !!t && !!PROJECT_DETAIL_TAB_SET[t];
    },

    /** Sync #projects / #projects/<id>/<tab>[/doc-or-decision-id] with UI (browser back/forward). */
    syncProjectsFromHash() {
      var path = (window.location.hash || '').replace(/^#\/?/, '');
      var seg = path.split('/').filter(Boolean);
      if (seg[0] !== 'projects') return;
      var pid = seg[1];
      var tab = seg[2];
      var subId = seg[3] != null && seg[3] !== '' ? decodeURIComponent(seg[3]) : null;
      var self = this;
      if (!pid) {
        if (this.selectedProject) {
          this.selectedProject = null;
          this.taskModalOpen = false;
          this.taskDetailHtml = '';
          this.backlogDetailTask = null;
          if (typeof this.resetBacklogSearch === 'function') this.resetBacklogSearch();
        }
        return;
      }
      if (!this.isValidProjectDetailTab(tab)) tab = 'overview';
      if (!this.projects.length) return;
      var proj = null;
      var i;
      for (i = 0; i < this.projects.length; i++) {
        if (String(this.projects[i].id) === String(pid)) {
          proj = this.projects[i];
          break;
        }
      }
      if (!proj) {
        this.selectedProject = null;
        this.taskModalOpen = false;
        this.taskDetailHtml = '';
        this.backlogDetailTask = null;
        if (typeof this.resetBacklogSearch === 'function') this.resetBacklogSearch();
        if (path !== 'projects') window.location.hash = 'projects';
        return;
      }
      if (
        this.selectedProject &&
        String(this.selectedProject.id) === String(pid) &&
        this.detailTab === tab
      ) {
        if (tab === 'docs' && subId) {
          if (!this.docsSelectedDoc || String(this.docsSelectedDoc.id) !== String(subId)) {
            if (typeof this.docsSelectDoc === 'function') void this.docsSelectDoc(subId);
          }
          return;
        }
        if (tab === 'decisions' && subId) {
          if (!this.decisionsSelected || String(this.decisionsSelected.id) !== String(subId)) {
            if (typeof this.decisionsSelectRow === 'function') void this.decisionsSelectRow({ id: subId });
          }
          return;
        }
        return;
      }
      this._backlogWsReloadNextAt = 0;
      this.selectedProject = proj;
      this.detailTab = tab;
      this.taskModalOpen = false;
      this.taskDetailHtml = '';
      this.backlogDetailTask = null;
      this.resetDetailCache();
      void Promise.resolve(this.loadDetailTab(tab)).then(function () {
        if (tab === 'docs' && subId && typeof self.docsSelectDoc === 'function') {
          return self.docsSelectDoc(subId);
        }
        if (tab === 'decisions' && subId && typeof self.decisionsSelectRow === 'function') {
          return self.decisionsSelectRow({ id: subId });
        }
      });
    },

    pushProjectsHash() {
      var raw = (window.location.hash || '').replace(/^#\/?/, '');
      var head = raw.split('/').filter(Boolean)[0] || '';
      // Stale fetches (docs/tab loads) finish after user navigates to e.g. #agents — do not rewrite hash.
      if (head !== 'projects') return;

      if (!this.selectedProject) {
        if (raw !== 'projects') {
          window.location.hash = 'projects';
        }
        return;
      }
      var t = this.detailTab;
      if (!this.isValidProjectDetailTab(t)) t = 'overview';
      var want =
        'projects/' +
        encodeURIComponent(this.selectedProject.id) +
        '/' +
        encodeURIComponent(t);
      if (t === 'docs' && this.docsSelectedDoc && this.docsSelectedDoc.id) {
        want += '/' + encodeURIComponent(String(this.docsSelectedDoc.id));
      }
      if (t === 'decisions' && this.decisionsSelected && this.decisionsSelected.id) {
        want += '/' + encodeURIComponent(String(this.decisionsSelected.id));
      }
      var cur = window.location.hash.replace(/^#\/?/, '');
      if (cur !== want) window.location.hash = want;
    },

    async onBacklogLiveUpdate(detail) {
      var routeHead =
        (window.location.hash || '').replace(/^#\/?/, '').split('/').filter(Boolean)[0] || '';
      if (routeHead !== 'projects') return;
      if (!detail || !this.selectedProject) return;
      if (String(detail.project_id) !== String(this.selectedProject.id)) return;
      var now = Date.now();
      if (now < this._backlogWsReloadNextAt) return;
      this._backlogWsReloadNextAt = now + 1200;
      var cur = this.detailTab;
      var silent =
        (cur === 'overview' && this._detailLoaded.overview) ||
        (cur === 'backlog' && this._detailLoaded.backlog) ||
        (cur === 'board' && this._boardLoaded) ||
        (cur === 'pert' && this._pertLoaded) ||
        (cur === 'docs' && this._detailLoaded.docs) ||
        (cur === 'decisions' && this._detailLoaded.decisions) ||
        (cur === 'milestones' && this._detailLoaded.milestones) ||
        (cur === 'agents' && this._detailLoaded.agents) ||
        (cur === 'pipelines' && this._detailLoaded.pipelines) ||
        (cur === 'workflows' && this._detailLoaded.workflows);
      var tabs = ['overview', 'backlog', 'board', 'pert', 'docs', 'decisions', 'milestones'];
      var i;
      for (i = 0; i < tabs.length; i++) {
        this.setDetailLoaded(tabs[i], false);
      }
      if (cur !== 'board' && typeof this.resetBoardCache === 'function') {
        this.resetBoardCache();
      }
      if (cur !== 'pert' && typeof this.resetPertCache === 'function') {
        this.resetPertCache();
      }
      await this.loadDetailTab(cur, true, silent);
    },

    setDetailLoaded(tab, done) {
      var d = Object.assign({}, this._detailLoaded);
      d[tab] = done;
      this._detailLoaded = d;
    },

    resetDetailCache() {
      this.projectOverview = null;
      this._detailLoaded = {
        overview: false,
        backlog: false,
        pert: false,
        spokes: false,
        agents: false,
        pipelines: false,
        workflows: false,
        docs: false,
        decisions: false,
        milestones: false,
      };
      this.detailTasks = [];
      this.detailSpokes = [];
      this.detailAgents = [];
      this.detailPipelines = [];
      this.projectWorkflows = [];
      this.wfRunModal = null;
      this.wfRunInput = '';
      this.wfRunError = '';
      this.bindAgentForm = { agent_id: '' };
      this.bindAgentError = '';
      this.detailErrors = {
        overview: '',
        backlog: '',
        spokes: '',
        agents: '',
        pipelines: '',
        workflows: '',
        docs: '',
        decisions: '',
        milestones: '',
        pert: '',
      };
      this.detailLoading = {
        overview: false,
        backlog: false,
        pert: false,
        spokes: false,
        agents: false,
        pipelines: false,
        workflows: false,
        docs: false,
        decisions: false,
        milestones: false,
      };
      if (typeof this.resetBoardCache === 'function') this.resetBoardCache();
      if (typeof this.resetPertCache === 'function') this.resetPertCache();
      if (typeof this.resetListFilters === 'function') this.resetListFilters();
      if (typeof this.resetDocsCache === 'function') this.resetDocsCache();
      if (typeof this.resetDecisionsCache === 'function') this.resetDecisionsCache();
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
      this.syncProjectsFromHash();
    },

    selectProject(project) {
      this._backlogWsReloadNextAt = 0;
      this.selectedProject = project;
      this.detailTab = 'overview';
      this.taskModalOpen = false;
      this.taskDetailHtml = '';
      this.backlogDetailTask = null;
      this.resetDetailCache();
      this.loadDetailTab('overview');
      this.pushProjectsHash();
    },

    backToList() {
      this._backlogWsReloadNextAt = 0;
      this.selectedProject = null;
      this.taskModalOpen = false;
      this.taskDetailHtml = '';
      this.backlogDetailTask = null;
      if (typeof this.resetBacklogSearch === 'function') this.resetBacklogSearch();
      this.pushProjectsHash();
    },

    retryLoad(tab) {
      this.setDetailLoaded(tab, false);
      this.loadDetailTab(tab, true);
    },

    overviewMapEntries(obj) {
      if (!obj || typeof obj !== 'object') return [];
      var keys = Object.keys(obj).sort();
      var out = [];
      var i;
      for (i = 0; i < keys.length; i++) out.push({ k: keys[i], v: obj[keys[i]] });
      return out;
    },

    async loadDetailTab(tab, force, silent) {
      if (!this.selectedProject) return;
      if (tab === 'overview') {
        if (!force && this._detailLoaded.overview) return;
        // `silent` is only set by onBacklogLiveUpdate; do not consult _detailLoaded here — that map is cleared just before this runs.
        var hideOv = !!silent;
        if (!hideOv) {
          this.setDetailLoading('overview', true);
          this.setDetailError('overview', '');
        }
        try {
          var pid = this.selectedProject.id;
          var base = '/api/projects/' + encodeURIComponent(pid);
          var results = await Promise.all([
            OpenFangAPI.get(base + '/backlog/overview'),
            OpenFangAPI.get(base + '/spokes'),
            OpenFangAPI.get(base + '/agents'),
            OpenFangAPI.get(base + '/pipelines?limit=50'),
          ]);
          this.projectOverview = results[0];
          this.detailSpokes = Array.isArray(results[1]) ? results[1] : [];
          this.detailAgents = Array.isArray(results[2]) ? results[2] : [];
          this.detailPipelines = Array.isArray(results[3]) ? results[3] : [];
          this.setDetailLoaded('overview', true);
          this.setDetailLoaded('spokes', true);
          this.setDetailLoaded('agents', true);
          this.setDetailLoaded('pipelines', true);
        } catch (e) {
          if (!hideOv) this.setDetailError('overview', e.message || 'Load failed');
          this.projectOverview = null;
        }
        if (!hideOv) this.setDetailLoading('overview', false);
        return;
      }
      if (tab === 'board') {
        await this.loadBoardData(!!force, !!silent);
        return;
      }
      if (tab === 'pert') {
        await this.loadPertTab(!!force, !!silent);
        return;
      }
      if (tab === 'docs') {
        await this.loadDocsTab(!!force, !!silent);
        return;
      }
      if (tab === 'decisions') {
        await this.loadDecisionsTab(!!force, !!silent);
        return;
      }
      if (tab === 'milestones') {
        await this.loadMilestonesTab(!!force, !!silent);
        return;
      }
      if (!force && this._detailLoaded[tab]) return;
      var pid = this.selectedProject.id;
      var hideSpinner =
        !!silent && (tab === 'backlog' || tab === 'agents' || tab === 'pipelines');
      if (!hideSpinner) {
        this.setDetailLoading(tab, true);
        this.setDetailError(tab, '');
      }
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
        } else if (tab === 'workflows') {
          var allWf = await OpenFangAPI.get('/api/workflows');
          var pidStr = String(pid);
          this.projectWorkflows = (Array.isArray(allWf) ? allWf : []).filter(function (w) {
            return w && String(w.project_id || '') === pidStr;
          });
        }
        this.setDetailLoaded(tab, true);
      } catch (e) {
        if (!hideSpinner) this.setDetailError(tab, e.message || 'Load failed');
      }
      if (!hideSpinner) this.setDetailLoading(tab, false);
    },

    async onDetailTabChange(tab) {
      this.detailTab = tab;
      await this.loadDetailTab(tab);
      this.pushProjectsHash();
    },

    async discoverSpokes() {
      if (!this.selectedProject) return;
      try {
        await OpenFangAPI.post('/api/projects/' + encodeURIComponent(this.selectedProject.id) + '/discover', {});
        this.setDetailLoaded('spokes', false);
        this.setDetailLoaded('overview', false);
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
        this.setDetailLoaded('overview', false);
        await this.loadDetailTab('agents', true);
      } catch (e) {
        this.bindAgentError = e.message || 'Bind failed';
      }
      this.bindAgentSubmitting = false;
    },

    openWfRunModal(wf) {
      if (!wf || !wf.id) return;
      this.wfRunModal = { id: wf.id, name: wf.name || wf.id };
      this.wfRunInput = '';
      this.wfRunError = '';
    },

    closeWfRunModal() {
      this.wfRunModal = null;
      this.wfRunInput = '';
      this.wfRunError = '';
    },

    async submitProjectWorkflowRun() {
      if (!this.selectedProject || !this.wfRunModal) return;
      this.wfRunSubmitting = true;
      this.wfRunError = '';
      try {
        await OpenFangAPI.post(
          '/api/projects/' +
            encodeURIComponent(this.selectedProject.id) +
            '/workflows/' +
            encodeURIComponent(this.wfRunModal.id) +
            '/run',
          { input: this.wfRunInput || '' }
        );
        OpenFangToast.success('Workflow finished');
        this.closeWfRunModal();
      } catch (e) {
        this.wfRunError = e.message || 'Run failed';
      }
      this.wfRunSubmitting = false;
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
        this.setDetailLoaded('overview', false);
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
    typeof backlogPertMixins === 'function' ? backlogPertMixins() : {},
    typeof backlogListMixins === 'function' ? backlogListMixins() : {},
    typeof backlogTaskDetailMixins === 'function' ? backlogTaskDetailMixins() : {},
    typeof backlogDocsMixins === 'function' ? backlogDocsMixins() : {},
    typeof backlogDecisionsMixins === 'function' ? backlogDecisionsMixins() : {},
    typeof backlogMilestonesMixins === 'function' ? backlogMilestonesMixins() : {},
    typeof backlogSearchMixins === 'function' ? backlogSearchMixins() : {}
  );
}

