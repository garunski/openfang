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
  mattermost: true,
};

/** Primary nav groups (UI); `detailTab` remains the concrete section key for routes + content. */
var PROJECT_DETAIL_TABS_BY_CATEGORY = {
  overview: ['overview'],
  tasks: ['backlog', 'board', 'pert', 'milestones'],
  knowledge: ['docs', 'decisions'],
  automation: ['spokes', 'agents', 'pipelines', 'workflows', 'mattermost'],
};

function projectDetailCategoryForTab(tab) {
  var c;
  for (c in PROJECT_DETAIL_TABS_BY_CATEGORY) {
    if (!Object.prototype.hasOwnProperty.call(PROJECT_DETAIL_TABS_BY_CATEGORY, c)) continue;
    var list = PROJECT_DETAIL_TABS_BY_CATEGORY[c];
    var i;
    for (i = 0; i < list.length; i++) {
      if (list[i] === tab) return c;
    }
  }
  return 'overview';
}

function projectDetailDefaultTabForCategory(cat) {
  if (cat === 'tasks') return 'board';
  var list = PROJECT_DETAIL_TABS_BY_CATEGORY[cat];
  return list && list.length ? list[0] : 'overview';
}

function projectsPage() {
  return Object.assign(
    {
    projects: [],
    projectsLoading: false,
    projectsError: '',
    selectedProject: null,
    detailTab: 'overview',
    /** @type {'overview'|'tasks'|'knowledge'|'automation'} */
    detailCategory: 'overview',
    projectOverview: null,
    detailTasks: [],
    detailSpokes: [],
    /** When set, URL is #projects/<id>/spokes/<name> and detail panel is open. */
    spokesDetailSpoke: null,
    spokeDetailLoading: false,
    spokeDetailError: '',
    spokeDetailRow: null,
    spokeGitLoading: false,
    spokeGitMutating: false,
    spokeGitError: '',
    spokeGitStatusPayload: null,
    spokeGitDiffText: '',
    spokeGitDiffTruncated: false,
    spokeGitDiffMaxBytes: 0,
    /** True after user (or refresh-after-mutation) fetched `/git/diff`. */
    spokeGitDiffLoaded: false,
    spokeGitDiffLoading: false,
    spokeGitBranchesPayload: null,
    /** `workspace` | `history` — hash #projects/.../spokes/<name>[/history[/sha]] */
    spokeDetailSubview: 'workspace',
    spokeHistoryCommitSha: null,
    spokeHistoryEntries: [],
    spokeHistoryLoading: false,
    spokeHistoryError: '',
    spokeHistoryDiffText: '',
    spokeHistoryDiffTruncated: false,
    spokeHistoryDiffMaxBytes: 0,
    spokeHistoryDiffLoaded: false,
    spokeHistoryDiffLoading: false,
    spokeHistoryDiffSideBySide: false,
    spokeDiffSideBySide: false,
    gitCommitMessage: '',
    gitBranchSwitchName: '',
    gitNewBranchName: '',
    /** Spokes topology map: drag background to pan (pixels). */
    topologyPanX: 0,
    topologyPanY: 0,
    topologyPanning: false,
    detailAgents: [],
    detailPipelines: [],
    projectWorkflows: [],
    wfRunModal: null,
    wfRunInput: '',
    wfRunSubmitting: false,
    wfRunError: '',
    wfRunsForWorkflow: null,
    wfRunsList: [],
    wfRunSelectedId: null,
    wfRunDetail: null,
    wfRunsLoading: false,
    wfRunDetailLoading: false,
    wfRunsPanelError: '',
    _wfRunPollTimer: null,
    mattermostForm: { channel_id: '', channel_name: '' },
    mattermostSaving: false,
    mattermostError: '',
    /** @type {{ id: string, name: string, state: string, missing?: boolean } | null} */
    _orchestratorAgentMeta: null,
    detailLoading: {
      overview: false,
      backlog: false,
      pert: false,
      spokes: false,
      agents: false,
      pipelines: false,
      workflows: false,
      mattermost: false,
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
      mattermost: '',
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
      mattermost: false,
      docs: false,
      decisions: false,
      milestones: false,
    },
    registerModalOpen: false,
    registerForm: { name: '', path: '', adminSpoke: '' },
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
      var extraSeg = seg[3] != null && seg[3] !== '' ? decodeURIComponent(seg[3]) : null;
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
        this.detailCategory = projectDetailCategoryForTab(tab);
        if (tab === 'docs' && extraSeg) {
          if (!this.docsSelectedDoc || String(this.docsSelectedDoc.id) !== String(extraSeg)) {
            if (typeof this.docsSelectDoc === 'function') void this.docsSelectDoc(extraSeg);
          }
          return;
        }
        if (tab === 'decisions' && extraSeg) {
          if (!this.decisionsSelected || String(this.decisionsSelected.id) !== String(extraSeg)) {
            if (typeof this.decisionsSelectRow === 'function') {
              void this.decisionsSelectRow({ id: extraSeg }, false, { openModal: true });
            }
          }
          return;
        }
        if (tab === 'spokes') {
          var sn =
            seg[3] != null && seg[3] !== '' ? decodeURIComponent(seg[3]) : null;
          var hist = seg[4] === 'history';
          var rawSha =
            hist && seg[5] != null && seg[5] !== ''
              ? decodeURIComponent(seg[5])
              : null;
          var wantHistSha =
            rawSha && /^[0-9a-fA-F]{4,64}$/.test(rawSha) ? rawSha : null;
          var wantSubview = hist ? 'history' : 'workspace';
          if (
            String(this.spokesDetailSpoke || '') === String(sn || '') &&
            String(this.spokeDetailSubview || 'workspace') === wantSubview &&
            String(this.spokeHistoryCommitSha || '') === String(wantHistSha || '')
          ) {
            return;
          }
          var spokeChanged = String(this.spokesDetailSpoke || '') !== String(sn || '');
          if (spokeChanged) {
            this.resetSpokeGitDiffOnly();
            this.resetSpokeHistoryUi();
          } else if (String(this.spokeDetailSubview || 'workspace') === 'history' && wantSubview === 'workspace') {
            this.resetSpokeHistoryUi();
          }
          this.spokesDetailSpoke = sn;
          this.spokeDetailSubview = wantSubview;
          this.spokeHistoryCommitSha = wantHistSha;
          if (!sn) {
            this.clearSpokeGitPanel();
            return;
          }
          void this.refreshSpokeDetail();
          return;
        }
        return;
      }
      this._backlogWsReloadNextAt = 0;
      this.selectedProject = proj;
      this.detailTab = tab;
      this.detailCategory = projectDetailCategoryForTab(tab);
      this.taskModalOpen = false;
      this.taskDetailHtml = '';
      this.backlogDetailTask = null;
      this.resetDetailCache();
      this.spokesDetailSpoke = tab === 'spokes' ? extraSeg || null : null;
      if (tab === 'spokes') {
        var hist0 = seg[4] === 'history';
        var rawSha0 =
          hist0 && seg[5] != null && seg[5] !== '' ? decodeURIComponent(seg[5]) : null;
        this.spokeDetailSubview = hist0 ? 'history' : 'workspace';
        this.spokeHistoryCommitSha =
          rawSha0 && /^[0-9a-fA-F]{4,64}$/.test(rawSha0) ? rawSha0 : null;
      } else {
        this.spokeDetailSubview = 'workspace';
        this.spokeHistoryCommitSha = null;
      }
      void Promise.resolve(this.loadDetailTab(tab)).then(function () {
        if (tab === 'docs' && extraSeg && typeof self.docsSelectDoc === 'function') {
          return self.docsSelectDoc(extraSeg);
        }
        if (tab === 'decisions' && extraSeg && typeof self.decisionsSelectRow === 'function') {
          return self.decisionsSelectRow({ id: extraSeg }, false, { openModal: true });
        }
        if (tab === 'spokes' && self.spokesDetailSpoke) {
          return self.refreshSpokeDetail();
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
      if (t === 'spokes' && this.spokesDetailSpoke) {
        want += '/' + encodeURIComponent(String(this.spokesDetailSpoke));
        if (this.spokeDetailSubview === 'history') {
          want += '/history';
          var hsha = this.spokeHistoryCommitSha;
          if (hsha && /^[0-9a-fA-F]{4,64}$/.test(String(hsha))) {
            want += '/' + encodeURIComponent(String(hsha));
          }
        }
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
        (cur === 'workflows' && this._detailLoaded.workflows) ||
        (cur === 'mattermost' && this._detailLoaded.mattermost);
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
        mattermost: false,
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
        mattermost: '',
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
        mattermost: false,
        docs: false,
        decisions: false,
        milestones: false,
      };
      this.mattermostForm = { channel_id: '', channel_name: '' };
      this.mattermostError = '';
      this._orchestratorAgentMeta = null;
      this.stopWorkflowRunPolling();
      this.wfRunsForWorkflow = null;
      this.wfRunsList = [];
      this.wfRunSelectedId = null;
      this.wfRunDetail = null;
      this.wfRunsLoading = false;
      this.wfRunDetailLoading = false;
      this.wfRunsPanelError = '';
      if (typeof this.resetBoardCache === 'function') this.resetBoardCache();
      if (typeof this.resetPertCache === 'function') this.resetPertCache();
      if (typeof this.resetListFilters === 'function') this.resetListFilters();
      if (typeof this.resetDocsCache === 'function') this.resetDocsCache();
      if (typeof this.resetDecisionsCache === 'function') this.resetDecisionsCache();
      if (typeof this.resetMilestonesCache === 'function') this.resetMilestonesCache();
      if (typeof this.resetBacklogSearch === 'function') this.resetBacklogSearch();
      this.backlogDetailTask = null;
      this.taskDetailEditMode = false;
      this.spokesDetailSpoke = null;
      this.clearSpokeGitPanel();
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
      this.detailCategory = 'overview';
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
      if (tab === 'mattermost') {
        var hideMm = !!silent;
        if (!hideMm) {
          this.setDetailLoading('mattermost', true);
          this.setDetailError('mattermost', '');
        }
        try {
          this.syncMattermostFormFromProject();
          await this.refreshOrchestratorMeta();
          this.setDetailLoaded('mattermost', true);
        } catch (e) {
          if (!hideMm) this.setDetailError('mattermost', e.message || 'Load failed');
        }
        if (!hideMm) this.setDetailLoading('mattermost', false);
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
          this.resetTopologyPan();
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
      if (typeof this.closeDecisionViewModal === 'function' && tab !== 'decisions') {
        this.closeDecisionViewModal();
      }
      if (tab === 'spokes') {
        this.resetTopologyPan();
      }
      if (tab !== 'spokes') {
        this.spokesDetailSpoke = null;
        this.clearSpokeGitPanel();
        this.resetTopologyPan();
      }
      if (tab !== 'workflows') {
        this.closeWorkflowRunsPanel();
      }
      this.detailTab = tab;
      this.detailCategory = projectDetailCategoryForTab(tab);
      await this.loadDetailTab(tab);
      this.pushProjectsHash();
    },

    async onDetailCategoryChange(category) {
      if (projectDetailCategoryForTab(this.detailTab) === category) return;
      var t = projectDetailDefaultTabForCategory(category);
      await this.onDetailTabChange(t);
    },

    projectDetailSubtabs() {
      return PROJECT_DETAIL_TABS_BY_CATEGORY[this.detailCategory] || ['overview'];
    },

    projectDetailSubtabLabel(tab) {
      var labels = {
        overview: 'Overview',
        backlog: 'List',
        board: 'Board',
        pert: 'PERT',
        milestones: 'Milestones',
        docs: 'Docs',
        decisions: 'Decisions',
        spokes: 'Spokes',
        agents: 'Agents',
        pipelines: 'Pipelines',
        workflows: 'Workflows',
        mattermost: 'Mattermost',
      };
      return labels[tab] || tab;
    },

    projectDetailSubtabIcon(tab) {
      var icons = {
        overview: 'fa-home',
        backlog: 'fa-list',
        board: 'fa-th',
        pert: 'fa-share-alt',
        milestones: 'fa-flag',
        docs: 'fa-file-text-o',
        decisions: 'fa-check-square-o',
        spokes: 'fa-code-fork',
        agents: 'fa-cog',
        pipelines: 'fa-terminal',
        workflows: 'fa-sitemap',
        mattermost: 'fa-comments',
      };
      return icons[tab] ? 'fa ' + icons[tab] : 'fa fa-circle-o';
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

    async setAdminSpoke(spokeName) {
      if (!this.selectedProject || !spokeName) return;
      try {
        await OpenFangAPI.put(
          '/api/projects/' + encodeURIComponent(this.selectedProject.id) + '/spokes/admin',
          { name: spokeName }
        );
        OpenFangToast.success('Hub updated');
        this.setDetailLoaded('spokes', false);
        this.setDetailLoaded('overview', false);
        await this.loadProjects();
        var pid = this.selectedProject.id;
        var proj = null;
        var i;
        for (i = 0; i < this.projects.length; i++) {
          if (String(this.projects[i].id) === String(pid)) {
            proj = this.projects[i];
            break;
          }
        }
        if (proj) this.selectedProject = proj;
        await this.loadDetailTab('spokes', true);
        if (this.detailTab === 'overview') await this.loadDetailTab('overview', true);
        if (this.spokesDetailSpoke) await this.refreshSpokeDetail();
      } catch (e) {
        this.setDetailError('spokes', e.message || 'Set Hub failed');
      }
    },

    resetSpokeGitDiffOnly() {
      this.spokeGitDiffLoaded = false;
      this.spokeGitDiffLoading = false;
      this.spokeGitDiffText = '';
      this.spokeGitDiffTruncated = false;
      this.spokeGitDiffMaxBytes = 0;
      var el = document.getElementById('spokeDiffContainer');
      if (el && el.shadowRoot) {
        var m = el.shadowRoot.querySelector('.d2h-mount');
        if (m) m.innerHTML = '';
      }
    },

    clearSpokeGitPanel() {
      this.spokeDetailLoading = false;
      this.spokeDetailError = '';
      this.spokeDetailRow = null;
      this.spokeGitLoading = false;
      this.spokeGitError = '';
      this.spokeGitStatusPayload = null;
      this.resetSpokeGitDiffOnly();
      this.spokeGitBranchesPayload = null;
      this.gitCommitMessage = '';
      this.gitBranchSwitchName = '';
      this.gitNewBranchName = '';
      this.resetSpokeHistoryState();
    },

    resetSpokeHistoryUi() {
      this.spokeHistoryCommitSha = null;
      this.spokeHistoryEntries = [];
      this.spokeHistoryLoading = false;
      this.spokeHistoryError = '';
      this.spokeHistoryDiffText = '';
      this.spokeHistoryDiffTruncated = false;
      this.spokeHistoryDiffMaxBytes = 0;
      this.spokeHistoryDiffLoaded = false;
      this.spokeHistoryDiffLoading = false;
      this.spokeHistoryDiffSideBySide = false;
      var hel = document.getElementById('spokeHistoryDiffContainer');
      if (hel && hel.shadowRoot) {
        var hm = hel.shadowRoot.querySelector('.d2h-mount');
        if (hm) hm.innerHTML = '';
      }
    },

    resetSpokeHistoryState() {
      this.spokeDetailSubview = 'workspace';
      this.resetSpokeHistoryUi();
    },

    spokeGitDirty() {
      var st = this.spokeGitStatusPayload && this.spokeGitStatusPayload.status;
      return !!(st && st.dirty);
    },

    /** Safe for Alpine x-for — avoids throws when API omits `files`. */
    spokeGitFilesList() {
      var s = this.spokeGitStatusPayload && this.spokeGitStatusPayload.status;
      var f = s && s.files;
      return Array.isArray(f) ? f : [];
    },

    spokeGitBranchesList() {
      var p = this.spokeGitBranchesPayload;
      var b = p && p.branches;
      return Array.isArray(b) ? b : [];
    },

    /** Spoke designated as Hub (`is_admin`); holds project `backlog/`. */
    hubSpokeFromList() {
      var list = this.detailSpokes || [];
      var i;
      for (i = 0; i < list.length; i++) {
        if (list[i].is_admin) return list[i];
      }
      return null;
    },

    nonHubSpokesFromList() {
      var list = this.detailSpokes || [];
      return list.filter(function (s) {
        return !s.is_admin;
      });
    },

    resetTopologyPan() {
      this.topologyPanX = 0;
      this.topologyPanY = 0;
      this.topologyPanning = false;
    },

    /**
     * Radial layout for topology SVG and nodes (fixed canvas size).
     * Rim = non-hub spokes (or all spokes if no Hub) plus one Discover slot.
     */
    spokeTopologyModel() {
      var W = 1100;
      var H = 800;
      var cx = W * 0.5;
      var cy = H * 0.5;
      var hub = this.hubSpokeFromList();
      var baseList = hub ? this.nonHubSpokesFromList() : (this.detailSpokes || []).slice();
      var rim = baseList.slice();
      rim.push({ __discover: true });
      var n = rim.length;
      var r =
        baseList.length === 0
          ? 220
          : Math.min(300, 155 + n * 34);
      var slots = [];
      var i;
      for (i = 0; i < n; i++) {
        var ang = (2 * Math.PI * i) / n - Math.PI / 2;
        var x = cx + r * Math.cos(ang);
        var y = cy + r * Math.sin(ang);
        if (rim[i].__discover) {
          slots.push({ discover: true, x: x, y: y });
        } else {
          slots.push({ spoke: rim[i], x: x, y: y });
        }
      }
      return { w: W, h: H, cx: cx, cy: cy, hub: hub, slots: slots };
    },

    /** Pan only; width/height/margins are fixed in CSS to match spokeTopologyModel(). */
    topologySurfaceStyle() {
      return (
        'transform:translate(' + this.topologyPanX + 'px,' + this.topologyPanY + 'px)'
      );
    },

    /**
     * Hub→rim segments as HTML (Alpine x-for inside &lt;svg&gt; is unreliable).
     * Each item: { key, style, muted }.
     */
    spokeTopologyEdges() {
      var m = this.spokeTopologyModel();
      var out = [];
      var i;
      for (i = 0; i < m.slots.length; i++) {
        var s = m.slots[i];
        var dx = s.x - m.cx;
        var dy = s.y - m.cy;
        var len = Math.sqrt(dx * dx + dy * dy);
        var angRad = Math.atan2(dy, dx);
        var angDeg = (angRad * 180) / Math.PI;
        var key = s.discover
          ? 'edge-discover-' + i
          : 'edge-' + i + '-' + String((s.spoke && s.spoke.name) || '') + '-' + String((s.spoke && s.spoke.path) || '');
        var style =
          'left:' +
          m.cx +
          'px;top:' +
          m.cy +
          'px;width:' +
          len +
          'px;height:0;transform-origin:0 0;transform:rotate(' +
          angDeg +
          'deg)';
        out.push({ key: key, style: style, muted: !!s.discover });
      }
      return out;
    },

    topologyViewportPointerDown(e) {
      if (e.pointerType === 'mouse' && e.button !== 0) return;
      var t = e.target;
      if (t && t.nodeType !== 1) t = t.parentElement;
      if (!t || typeof t.closest !== 'function') return;
      if (t.closest('.of-topology-node')) return;
      this.topologyPanning = true;
      var startX = e.clientX;
      var startY = e.clientY;
      var origX = this.topologyPanX;
      var origY = this.topologyPanY;
      var self = this;
      function onMove(ev) {
        if (!self.topologyPanning) return;
        self.topologyPanX = origX + (ev.clientX - startX);
        self.topologyPanY = origY + (ev.clientY - startY);
      }
      function onUp() {
        self.topologyPanning = false;
        window.removeEventListener('pointermove', onMove);
        window.removeEventListener('pointerup', onUp);
        window.removeEventListener('pointercancel', onUp);
      }
      window.addEventListener('pointermove', onMove);
      window.addEventListener('pointerup', onUp);
      window.addEventListener('pointercancel', onUp);
    },

    openSpokeDetail(s) {
      if (!s || !s.name) return;
      this.detailTab = 'spokes';
      this.detailCategory = 'automation';
      this.spokeDetailSubview = 'workspace';
      this.resetSpokeHistoryUi();
      this.resetSpokeGitDiffOnly();
      this.spokesDetailSpoke = s.name;
      this.pushProjectsHash();
      void this.refreshSpokeDetail();
    },

    /** Prefer browser history so Back matches this control; fallback if there is no prior entry. */
    closeSpokeDetail() {
      if (!this.selectedProject || !this.spokesDetailSpoke) return;
      var listHash =
        'projects/' +
        encodeURIComponent(this.selectedProject.id) +
        '/spokes';
      var cur = (window.location.hash || '').replace(/^#\/?/, '');
      if (cur === listHash) {
        this.spokesDetailSpoke = null;
        this.clearSpokeGitPanel();
        return;
      }
      var self = this;
      window.history.back();
      setTimeout(function () {
        var h = (window.location.hash || '').replace(/^#\/?/, '');
        if (h !== listHash && self.spokesDetailSpoke) {
          self.spokesDetailSpoke = null;
          self.clearSpokeGitPanel();
          window.location.hash = listHash;
        }
      }, 120);
    },

    async refreshSpokeDetail() {
      if (!this.selectedProject || !this.spokesDetailSpoke) return;
      this.spokeDetailLoading = true;
      this.spokeDetailError = '';
      var pid = this.selectedProject.id;
      var base =
        '/api/projects/' + encodeURIComponent(pid) + '/spokes/' + encodeURIComponent(this.spokesDetailSpoke);
      try {
        this.spokeDetailRow = await OpenFangAPI.get(base);
        await this.reloadSpokeGitReads();
      } catch (e) {
        this.spokeDetailError = e.message || 'Failed to load spoke';
      }
      this.spokeDetailLoading = false;
    },

    async reloadSpokeGitReads() {
      if (!this.selectedProject || !this.spokesDetailSpoke) return;
      this.spokeGitLoading = true;
      this.spokeGitError = '';
      var pid = this.selectedProject.id;
      var sn = encodeURIComponent(this.spokesDetailSpoke);
      var base = '/api/projects/' + encodeURIComponent(pid) + '/spokes/' + sn + '/git';
      try {
        this.spokeGitStatusPayload = await OpenFangAPI.get(base + '/status');
        this.spokeGitBranchesPayload = await OpenFangAPI.get(base + '/branches');
      } catch (e) {
        this.spokeGitError = e.message || 'Git load failed';
      }
      this.spokeGitLoading = false;
      if (this.spokeGitError) return;
      if (this.spokeDetailSubview === 'history') {
        if (this.spokeHistoryCommitSha) {
          await this.loadSpokeHistoryCommitDiff(this.spokeHistoryCommitSha);
        } else {
          await this.loadSpokeGitLog();
        }
        return;
      }
      if (this.spokeGitDiffLoaded) {
        await this.fetchSpokeGitDiffContent();
      } else {
        this.resetSpokeGitDiffOnly();
        await this.renderSpokeDiff();
      }
    },

    /** Fetches `/git/diff` and renders. Caller sets `spokeGitDiffLoaded` for first load. */
    async fetchSpokeGitDiffContent() {
      if (!this.selectedProject || !this.spokesDetailSpoke) return;
      this.spokeGitDiffLoading = true;
      var pid = this.selectedProject.id;
      var sn = encodeURIComponent(this.spokesDetailSpoke);
      var base = '/api/projects/' + encodeURIComponent(pid) + '/spokes/' + sn + '/git';
      try {
        var df = await OpenFangAPI.get(base + '/diff');
        this.spokeGitDiffText = (df && df.unified_diff) || '';
        this.spokeGitDiffTruncated = !!(df && df.diff_truncated);
        this.spokeGitDiffMaxBytes = df && df.diff_max_bytes != null ? Number(df.diff_max_bytes) : 0;
        await this.renderSpokeDiff();
      } catch (e) {
        OpenFangToast.error(e.message || 'Diff load failed');
        this.spokeGitDiffLoaded = false;
        this.spokeGitDiffText = '';
        this.spokeGitDiffTruncated = false;
        this.spokeGitDiffMaxBytes = 0;
        await this.renderSpokeDiff();
      }
      this.spokeGitDiffLoading = false;
    },

    async loadSpokeGitDiff() {
      if (!this.selectedProject || !this.spokesDetailSpoke || this.spokeGitDiffLoading || this.spokeGitLoading) {
        return;
      }
      this.spokeGitDiffLoaded = true;
      await this.fetchSpokeGitDiffContent();
    },

    refreshSpokeGitPanel() {
      void this.reloadSpokeGitReads();
    },

    openSpokeHistory() {
      if (!this.selectedProject || !this.spokesDetailSpoke) return;
      this.spokeDetailSubview = 'history';
      this.resetSpokeHistoryUi();
      this.pushProjectsHash();
      void this.reloadSpokeGitReads();
    },

    backSpokeHistoryList() {
      if (!this.selectedProject || !this.spokesDetailSpoke) return;
      this.spokeHistoryCommitSha = null;
      this.spokeHistoryDiffText = '';
      this.spokeHistoryDiffTruncated = false;
      this.spokeHistoryDiffMaxBytes = 0;
      this.spokeHistoryDiffLoaded = false;
      this.spokeHistoryDiffLoading = false;
      void this.renderUnifiedDiffIntoHost(
        'spokeHistoryDiffContainer',
        '',
        this.spokeHistoryDiffSideBySide
      );
      this.pushProjectsHash();
    },

    backSpokeWorkspaceFromHistory() {
      if (!this.selectedProject || !this.spokesDetailSpoke) return;
      this.spokeDetailSubview = 'workspace';
      this.resetSpokeHistoryUi();
      this.pushProjectsHash();
      void this.reloadSpokeGitReads();
    },

    selectSpokeHistoryCommit(entry) {
      if (!entry || !entry.oid) return;
      this.spokeHistoryCommitSha = entry.oid;
      this.pushProjectsHash();
      void this.loadSpokeHistoryCommitDiff(entry.oid);
    },

    async loadSpokeGitLog() {
      if (!this.selectedProject || !this.spokesDetailSpoke) return;
      this.spokeHistoryLoading = true;
      this.spokeHistoryError = '';
      var pid = this.selectedProject.id;
      var sn = encodeURIComponent(this.spokesDetailSpoke);
      var url =
        '/api/projects/' + encodeURIComponent(pid) + '/spokes/' + sn + '/git/log?limit=50';
      try {
        var data = await OpenFangAPI.get(url);
        if (data && data.is_git_repo) {
          this.spokeHistoryEntries = Array.isArray(data.commits) ? data.commits : [];
        } else {
          this.spokeHistoryEntries = [];
        }
      } catch (e) {
        this.spokeHistoryError = e.message || 'Log failed';
        this.spokeHistoryEntries = [];
      }
      this.spokeHistoryLoading = false;
    },

    async loadSpokeHistoryCommitDiff(sha) {
      if (!this.selectedProject || !this.spokesDetailSpoke || !sha) return;
      if (!/^[0-9a-fA-F]{4,64}$/.test(String(sha))) {
        this.spokeHistoryError = 'Invalid revision';
        return;
      }
      this.spokeHistoryDiffLoading = true;
      this.spokeHistoryError = '';
      var pid = this.selectedProject.id;
      var sn = encodeURIComponent(this.spokesDetailSpoke);
      var base = '/api/projects/' + encodeURIComponent(pid) + '/spokes/' + sn + '/git';
      try {
        var df = await OpenFangAPI.get(base + '/commit/' + encodeURIComponent(sha) + '/diff');
        this.spokeHistoryDiffText = (df && df.unified_diff) || '';
        this.spokeHistoryDiffTruncated = !!(df && df.diff_truncated);
        this.spokeHistoryDiffMaxBytes =
          df && df.diff_max_bytes != null ? Number(df.diff_max_bytes) : 0;
        this.spokeHistoryDiffLoaded = true;
        await this.renderUnifiedDiffIntoHost(
          'spokeHistoryDiffContainer',
          this.spokeHistoryDiffText || '',
          this.spokeHistoryDiffSideBySide
        );
      } catch (e) {
        OpenFangToast.error(e.message || 'Commit diff failed');
        this.spokeHistoryDiffLoaded = false;
        this.spokeHistoryDiffText = '';
        this.spokeHistoryDiffTruncated = false;
        this.spokeHistoryDiffMaxBytes = 0;
        await this.renderUnifiedDiffIntoHost('spokeHistoryDiffContainer', '', this.spokeHistoryDiffSideBySide);
      }
      this.spokeHistoryDiffLoading = false;
    },

    async renderUnifiedDiffIntoHost(hostId, text, sideBySide) {
      var self = this;
      await new Promise(function (resolve) {
        if (typeof self.$nextTick === 'function') self.$nextTick(resolve);
        else queueMicrotask(resolve);
      });
      var el = document.getElementById(hostId);
      if (!el) return;
      el.classList.add('of-diff2html-wrap');
      var UI = typeof Diff2HtmlUI !== 'undefined' ? Diff2HtmlUI : window.Diff2HtmlUI;
      var D2H_LAYOUT_FIX =
        '.d2h-code-wrapper{position:relative;overflow-x:auto;max-width:100%;}' +
        '.d2h-files-diff .d2h-file-side-diff{vertical-align:top;}' +
        '.d2h-diff-table td{vertical-align:top;}';
      function clearMount() {
        if (el.shadowRoot) {
          var m = el.shadowRoot.querySelector('.d2h-mount');
          if (m) m.innerHTML = '';
        }
      }
      if (!String(text).trim()) {
        clearMount();
        return;
      }
      try {
        var sr = el.shadowRoot;
        if (!sr) {
          sr = el.attachShadow({ mode: 'open' });
          var link = document.createElement('link');
          link.rel = 'stylesheet';
          link.href = '/vendor/diff2html/diff2html.min.css';
          sr.appendChild(link);
          await new Promise(function (resolve, reject) {
            link.onload = function () {
              resolve();
            };
            link.onerror = function () {
              reject(new Error('diff2html css'));
            };
          });
        }
        if (!sr.querySelector('style[data-of-d2h-fix]')) {
          var fixStyle = document.createElement('style');
          fixStyle.setAttribute('data-of-d2h-fix', '1');
          fixStyle.textContent = D2H_LAYOUT_FIX;
          var mountRef = sr.querySelector('.d2h-mount');
          if (mountRef) sr.insertBefore(fixStyle, mountRef);
          else sr.appendChild(fixStyle);
        }
        var mount = sr.querySelector('.d2h-mount');
        if (!mount) {
          mount = document.createElement('div');
          mount.className = 'd2h-mount';
          sr.appendChild(mount);
        }
        mount.innerHTML = '';
        if (!UI) {
          mount.textContent = text;
          return;
        }
        var fmt = sideBySide ? 'side-by-side' : 'line-by-line';
        var ui = new UI(mount, text, {
          drawFileList: true,
          matching: 'lines',
          outputFormat: fmt,
        });
        ui.draw();
        if (typeof ui.highlightCode === 'function') ui.highlightCode();
      } catch (e) {
        clearMount();
        if (!el.shadowRoot) {
          el.textContent = text;
          return;
        }
        var m = el.shadowRoot.querySelector('.d2h-mount');
        if (!m) {
          m = document.createElement('div');
          m.className = 'd2h-mount';
          el.shadowRoot.appendChild(m);
        }
        m.textContent = text;
      }
    },

    async renderSpokeDiff() {
      await this.renderUnifiedDiffIntoHost(
        'spokeDiffContainer',
        this.spokeGitDiffText || '',
        this.spokeDiffSideBySide
      );
    },

    async toggleSpokeDiffLayout() {
      if (!this.spokeGitDiffLoaded || !String(this.spokeGitDiffText || '').trim()) return;
      this.spokeDiffSideBySide = !this.spokeDiffSideBySide;
      await this.renderSpokeDiff();
    },

    async toggleSpokeHistoryDiffLayout() {
      if (!this.spokeHistoryDiffLoaded || !String(this.spokeHistoryDiffText || '').trim()) return;
      this.spokeHistoryDiffSideBySide = !this.spokeHistoryDiffSideBySide;
      await this.renderUnifiedDiffIntoHost(
        'spokeHistoryDiffContainer',
        this.spokeHistoryDiffText || '',
        this.spokeHistoryDiffSideBySide
      );
    },

    async gitStageFile(path) {
      if (!this.selectedProject || !this.spokesDetailSpoke || !path || this.spokeGitMutating) return;
      this.spokeGitMutating = true;
      try {
        var pid = this.selectedProject.id;
        var base =
          '/api/projects/' +
          encodeURIComponent(pid) +
          '/spokes/' +
          encodeURIComponent(this.spokesDetailSpoke) +
          '/git/stage';
        await OpenFangAPI.post(base, { path: path });
        await this.reloadSpokeGitReads();
      } catch (e) {
        OpenFangToast.error(e.message || 'Stage failed');
      }
      this.spokeGitMutating = false;
    },

    async gitUnstageFile(path) {
      if (!this.selectedProject || !this.spokesDetailSpoke || !path || this.spokeGitMutating) return;
      this.spokeGitMutating = true;
      try {
        var pid = this.selectedProject.id;
        var base =
          '/api/projects/' +
          encodeURIComponent(pid) +
          '/spokes/' +
          encodeURIComponent(this.spokesDetailSpoke) +
          '/git/unstage';
        await OpenFangAPI.post(base, { path: path });
        await this.reloadSpokeGitReads();
      } catch (e) {
        OpenFangToast.error(e.message || 'Unstage failed');
      }
      this.spokeGitMutating = false;
    },

    async gitStageAllSpoke() {
      if (!this.selectedProject || !this.spokesDetailSpoke || this.spokeGitMutating) return;
      this.spokeGitMutating = true;
      try {
        var pid = this.selectedProject.id;
        var base =
          '/api/projects/' +
          encodeURIComponent(pid) +
          '/spokes/' +
          encodeURIComponent(this.spokesDetailSpoke) +
          '/git/stage-all';
        await OpenFangAPI.post(base, {});
        await this.reloadSpokeGitReads();
      } catch (e) {
        OpenFangToast.error(e.message || 'Stage all failed');
      }
      this.spokeGitMutating = false;
    },

    async gitCommitSpoke() {
      if (!this.selectedProject || !this.spokesDetailSpoke || this.spokeGitMutating) return;
      var msg = (this.gitCommitMessage || '').trim();
      if (!msg) {
        OpenFangToast.error('Commit message required');
        return;
      }
      this.spokeGitMutating = true;
      try {
        var pid = this.selectedProject.id;
        var base =
          '/api/projects/' +
          encodeURIComponent(pid) +
          '/spokes/' +
          encodeURIComponent(this.spokesDetailSpoke) +
          '/git/commit';
        await OpenFangAPI.post(base, { message: msg });
        this.gitCommitMessage = '';
        OpenFangToast.success('Committed');
        await this.reloadSpokeGitReads();
      } catch (e) {
        OpenFangToast.error(e.message || 'Commit failed');
      }
      this.spokeGitMutating = false;
    },

    async gitCheckoutSpoke(create) {
      if (!this.selectedProject || !this.spokesDetailSpoke || this.spokeGitMutating) return;
      var name = create
        ? (this.gitNewBranchName || '').trim()
        : (this.gitBranchSwitchName || '').trim();
      if (!name) {
        OpenFangToast.error('Branch name required');
        return;
      }
      if (this.spokeGitDirty() && !create) {
        if (!window.confirm('Working tree has local changes. Switch branch anyway?')) return;
      }
      this.spokeGitMutating = true;
      try {
        var pid = this.selectedProject.id;
        var base =
          '/api/projects/' +
          encodeURIComponent(pid) +
          '/spokes/' +
          encodeURIComponent(this.spokesDetailSpoke) +
          '/git/checkout';
        await OpenFangAPI.post(base, { branch: name, create: !!create });
        if (create) this.gitNewBranchName = '';
        OpenFangToast.success(create ? 'Branch created' : 'Switched branch');
        await this.reloadSpokeGitReads();
      } catch (e) {
        OpenFangToast.error(e.message || 'Checkout failed');
      }
      this.spokeGitMutating = false;
    },

    async gitPushSpoke() {
      if (!this.selectedProject || !this.spokesDetailSpoke || this.spokeGitMutating) return;
      if (!window.confirm('Push to configured remote?')) return;
      this.spokeGitMutating = true;
      try {
        var pid = this.selectedProject.id;
        var base =
          '/api/projects/' +
          encodeURIComponent(pid) +
          '/spokes/' +
          encodeURIComponent(this.spokesDetailSpoke) +
          '/git/push';
        await OpenFangAPI.post(base, {});
        OpenFangToast.success('Push finished');
        await this.reloadSpokeGitReads();
      } catch (e) {
        OpenFangToast.error(e.message || 'Push failed');
      }
      this.spokeGitMutating = false;
    },

    openRegisterModal() {
      this.registerModalOpen = true;
      this.registerError = '';
      this.registerForm = { name: '', path: '', adminSpoke: '' };
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
        var body = {
          name: this.registerForm.name.trim(),
          path: this.registerForm.path.trim(),
        };
        var as = (this.registerForm.adminSpoke || '').trim();
        if (as) body.admin_spoke = as;
        await OpenFangAPI.post('/api/projects', body);
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

    stopWorkflowRunPolling() {
      if (this._wfRunPollTimer) {
        clearInterval(this._wfRunPollTimer);
        this._wfRunPollTimer = null;
      }
    },

    closeWorkflowRunsPanel() {
      this.stopWorkflowRunPolling();
      this.wfRunsForWorkflow = null;
      this.wfRunsList = [];
      this.wfRunSelectedId = null;
      this.wfRunDetail = null;
      this.wfRunsLoading = false;
      this.wfRunDetailLoading = false;
      this.wfRunsPanelError = '';
    },

    async openWorkflowRunsPanel(wf) {
      if (!wf || !wf.id) return;
      this.wfRunsForWorkflow = { id: wf.id, name: wf.name || wf.id };
      this.wfRunSelectedId = null;
      this.wfRunDetail = null;
      this.wfRunsPanelError = '';
      this.stopWorkflowRunPolling();
      this.wfRunsLoading = true;
      try {
        this.wfRunsList = await OpenFangAPI.get(
          '/api/workflows/' + encodeURIComponent(wf.id) + '/runs'
        );
        if (!Array.isArray(this.wfRunsList)) this.wfRunsList = [];
      } catch (e) {
        this.wfRunsList = [];
        this.wfRunsPanelError = e.message || 'Failed to load runs';
      }
      this.wfRunsLoading = false;
    },

    async selectWorkflowRun(run) {
      if (!run || !run.id || !this.wfRunsForWorkflow) return;
      this.wfRunSelectedId = run.id;
      this.stopWorkflowRunPolling();
      await this.fetchWorkflowRunDetail(false);
      if (this.wfRunDetail && String(this.wfRunDetail.state || '').toLowerCase() === 'running') {
        this.startWorkflowRunPolling();
      }
    },

    async fetchWorkflowRunDetail(silent) {
      if (!this.wfRunsForWorkflow || !this.wfRunSelectedId) return;
      if (!silent) this.wfRunDetailLoading = true;
      try {
        this.wfRunDetail = await OpenFangAPI.get(
          '/api/workflows/' +
            encodeURIComponent(this.wfRunsForWorkflow.id) +
            '/runs/' +
            encodeURIComponent(this.wfRunSelectedId)
        );
      } catch (e) {
        this.wfRunDetail = null;
        if (!silent) OpenFangToast.error(e.message || 'Run detail failed');
      }
      if (!silent) this.wfRunDetailLoading = false;
    },

    startWorkflowRunPolling() {
      var self = this;
      this.stopWorkflowRunPolling();
      this._wfRunPollTimer = setInterval(function () {
        void self.fetchWorkflowRunDetail(true).then(function () {
          if (!self.wfRunDetail || String(self.wfRunDetail.state || '').toLowerCase() !== 'running') {
            self.stopWorkflowRunPolling();
          }
        });
      }, 3000);
    },

    formatWfDurationMs(ms) {
      if (ms == null || ms === '') return '—';
      var n = Number(ms);
      if (!isFinite(n)) return '—';
      if (n < 1000) return n + ' ms';
      return (n / 1000).toFixed(1) + ' s';
    },

    wfRunStateBadgeClass(st) {
      var x = String(st || '').toLowerCase();
      if (x === 'completed') return 'badge-success';
      if (x === 'failed') return 'badge-error';
      if (x === 'running') return 'badge-info';
      return 'badge-dim';
    },

    wfRunStepOutputPreview(text, maxLen) {
      var m = maxLen || 280;
      var s = text == null ? '' : String(text);
      if (s.length <= m) return s;
      return s.slice(0, m) + '\u2026';
    },

    wfRunLastStepName() {
      var r = this.wfRunDetail;
      if (!r || !Array.isArray(r.step_results) || !r.step_results.length) return '';
      var last = r.step_results[r.step_results.length - 1];
      return last && last.step_name ? String(last.step_name) : '';
    },

    async submitProjectWorkflowRun() {
      if (!this.selectedProject || !this.wfRunModal) return;
      this.wfRunSubmitting = true;
      this.wfRunError = '';
      try {
        var wid = this.wfRunModal.id;
        var wname = this.wfRunModal.name;
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
        if (this.wfRunsForWorkflow && String(this.wfRunsForWorkflow.id) === String(wid)) {
          await this.openWorkflowRunsPanel({ id: wid, name: wname || wid });
        }
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

    syncMattermostFormFromProject() {
      var p = this.selectedProject;
      if (!p) {
        this.mattermostForm = { channel_id: '', channel_name: '' };
        return;
      }
      this.mattermostForm = {
        channel_id: p.mattermost_channel_id != null ? String(p.mattermost_channel_id) : '',
        channel_name: p.mattermost_channel_name != null ? String(p.mattermost_channel_name) : '',
      };
    },

    async refreshOrchestratorMeta() {
      this._orchestratorAgentMeta = null;
      var p = this.selectedProject;
      if (!p || !p.orchestrator_agent_id) return;
      var oid = String(p.orchestrator_agent_id);
      await Alpine.store('app').refreshAgents();
      var agents = Alpine.store('app').agents || [];
      var i;
      for (i = 0; i < agents.length; i++) {
        if (String(agents[i].id) === oid) {
          this._orchestratorAgentMeta = {
            id: agents[i].id,
            name: agents[i].name || '',
            state: agents[i].state != null ? String(agents[i].state) : '',
          };
          return;
        }
      }
      try {
        var d = await OpenFangAPI.get('/api/agents/' + encodeURIComponent(oid));
        this._orchestratorAgentMeta = {
          id: oid,
          name: d.name != null ? String(d.name) : '',
          state: d.state != null ? String(d.state) : '',
        };
      } catch (e) {
        this._orchestratorAgentMeta = { id: oid, name: '', state: '', missing: true };
      }
    },

    mattermostBindingSummary() {
      var p = this.selectedProject;
      if (!p) return 'Not configured';
      var id = p.mattermost_channel_id;
      var name = p.mattermost_channel_name;
      var hasId = id != null && String(id).trim() !== '';
      var hasName = name != null && String(name).trim() !== '';
      if (!hasId && !hasName) return 'Not configured';
      var parts = [];
      if (hasName) parts.push(String(name).trim());
      if (hasId) parts.push(String(id).trim());
      return parts.join(' — ');
    },

    orchestratorIsActive() {
      var m = this._orchestratorAgentMeta;
      if (!m || m.missing) return false;
      return String(m.state || '').toLowerCase().indexOf('running') >= 0;
    },

    orchestratorDisplayName() {
      var m = this._orchestratorAgentMeta;
      if (!m) return '';
      if (m.name && String(m.name).trim()) return String(m.name).trim();
      return m.id || '';
    },

    async saveMattermostBinding() {
      if (!this.selectedProject || this.mattermostSaving) return;
      this.mattermostSaving = true;
      this.mattermostError = '';
      try {
        var cid = (this.mattermostForm.channel_id || '').trim();
        var cname = (this.mattermostForm.channel_name || '').trim();
        await OpenFangAPI.put('/api/projects/' + encodeURIComponent(this.selectedProject.id), {
          mattermost_channel_id: cid ? cid : null,
          mattermost_channel_name: cname ? cname : null,
        });
        OpenFangToast.success('Mattermost channel saved');
        this.setDetailLoaded('overview', false);
        await this.loadProjects();
        var pid = this.selectedProject.id;
        var proj = null;
        var j;
        for (j = 0; j < this.projects.length; j++) {
          if (String(this.projects[j].id) === String(pid)) {
            proj = this.projects[j];
            break;
          }
        }
        if (proj) this.selectedProject = proj;
        this.syncMattermostFormFromProject();
        await this.refreshOrchestratorMeta();
        if (this.detailTab === 'overview') await this.loadDetailTab('overview', true);
      } catch (e) {
        this.mattermostError = e.message || 'Save failed';
      }
      this.mattermostSaving = false;
    },

    async clearMattermostBinding() {
      if (!this.selectedProject || this.mattermostSaving) return;
      this.mattermostForm.channel_id = '';
      this.mattermostForm.channel_name = '';
      await this.saveMattermostBinding();
    },

    async openOrchestratorAgentDetail() {
      var oid = this.selectedProject && this.selectedProject.orchestrator_agent_id;
      if (!oid) return;
      await Alpine.store('app').refreshAgents();
      var agents = Alpine.store('app').agents || [];
      var found = null;
      var k;
      for (k = 0; k < agents.length; k++) {
        if (String(agents[k].id) === String(oid)) {
          found = agents[k];
          break;
        }
      }
      if (!found) {
        try {
          found = await OpenFangAPI.get('/api/agents/' + encodeURIComponent(String(oid)));
        } catch (e) {
          OpenFangToast.warn('Could not load orchestrator agent.');
          return;
        }
      }
      Alpine.store('app').pendingShowAgentDetail = found;
      if (this.$root && typeof this.$root.navigate === 'function') {
        this.$root.navigate('agents/sessions');
      } else {
        window.location.hash = 'agents/sessions';
      }
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

