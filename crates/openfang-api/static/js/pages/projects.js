// Projects page — list, register, project detail (backlog, spokes, agents, workflow runs)
'use strict';

var PROJECT_DETAIL_TAB_SET = {
  overview: true,
  orchestrator: true,
  agents: true,
  backlog: true,
  board: true,
  pert: true,
  docs: true,
  decisions: true,
  milestones: true,
  spokes: true,
  workflow_runs: true,
  project_agents: true,
  mattermost: true,
};

/** Primary nav groups (UI); `detailTab` remains the concrete section key for routes + content. */
var PROJECT_DETAIL_TABS_BY_CATEGORY = {
  overview: ['overview', 'orchestrator', 'agents'],
  tasks: ['backlog', 'board', 'pert', 'milestones'],
  knowledge: ['docs', 'decisions'],
  automation: ['project_agents', 'spokes', 'workflow_runs', 'mattermost'],
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
  if (cat === 'automation') return 'project_agents';
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
    /** Workflow engine runs for this project (`GET .../conduit-runs`). */
    detailConduitRuns: [],
    /** Selected run id (summary row) for highlight + refetch. */
    conduitRunSelectedId: null,
    /** Conduit id for the selected row (retry detail fetch when full payload missing). */
    conduitRunSelectedConduitId: null,
    /** Full run payload from `GET /api/conduits/:id/runs/:run_id`. */
    conduitRunDetail: null,
    conduitRunDetailLoading: false,
    conduitRunDetailError: '',
    conduitRunTraceModalOpen: false,
    conduitRunTraceLoading: false,
    conduitRunTraceError: '',
    conduitRunTraceContent: '',
    conduitRunTraceNote: '',
    /** @type {{ orchestrator_agent_id: string|null, current: any[], historical: any[] } | null} */
    projectAgentsManagement: null,
    orchestratorStartSubmitting: false,
    mattermostForm: { team_name: '', channel_name: '' },
    mattermostSaving: false,
    mattermostTestSending: false,
    mattermostError: '',
    /** @type {{ id: string, name: string, state: string, missing?: boolean } | null} */
    _orchestratorAgentMeta: null,
    /** Project-scoped agents for Chat tab (`GET /api/projects/:id/agents`). */
    projectChatAgents: [],
    projectChatAgentsLoading: false,
    /** When set, inline `chatPageProjectEmbed()` is mounted for that agent. */
    projectChatSelectedAgentId: null,
    detailLoading: {
      overview: false,
      agents: false,
      backlog: false,
      pert: false,
      spokes: false,
      workflow_runs: false,
      project_agents: false,
      mattermost: false,
      docs: false,
      decisions: false,
      milestones: false,
    },
    detailErrors: {
      overview: '',
      agents: '',
      backlog: '',
      pert: '',
      spokes: '',
      workflow_runs: '',
      project_agents: '',
      mattermost: '',
      docs: '',
      decisions: '',
      milestones: '',
    },
    _detailLoaded: {
      overview: false,
      agents: false,
      backlog: false,
      pert: false,
      spokes: false,
      workflow_runs: false,
      project_agents: false,
      mattermost: false,
      docs: false,
      decisions: false,
      milestones: false,
    },
    registerModalOpen: false,
    registerForm: { name: '', path: '', adminSpoke: '' },
    registerSubmitting: false,
    registerError: '',
    taskModalOpen: false,
    taskModalTitle: '',
    taskModalLoading: false,
    taskModalError: '',
    taskDetailHtml: '',
    /** Min ms between WS-driven backlog refreshes (matches server watch debounce; avoids refetch storms). */
    _backlogWsReloadNextAt: 0,
    _agentKilledBound: false,

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
      if (!this._agentKilledBound) {
        this._agentKilledBound = true;
        window.addEventListener('openfang-agent-killed', function (ev) {
          var id = ev.detail && ev.detail.agentId != null ? String(ev.detail.agentId) : '';
          void self.onAgentKilledFromHub(id);
        });
      }
    },

    /** After DELETE /api/agents/:id — refresh project rows and drop stale orchestrator/chat selection. */
    async onAgentKilledFromHub(agentId) {
      if (!agentId) return;
      await Alpine.store('app').refreshAgents();
      if (
        this.projectChatSelectedAgentId &&
        String(this.projectChatSelectedAgentId) === agentId
      ) {
        this.clearProjectChatAgent();
      }
      if (
        this.selectedProject &&
        this.selectedProject.orchestrator_agent_id != null &&
        String(this.selectedProject.orchestrator_agent_id) === agentId
      ) {
        this.selectedProject.orchestrator_agent_id = null;
      }
      this._orchestratorAgentMeta = null;
      this.setDetailLoaded('overview', false);
      this.setDetailLoaded('agents', false);
      this.setDetailLoaded('project_agents', false);
      this.setDetailLoaded('mattermost', false);
      try {
        await this.loadProjects();
      } catch (e) {
        /* ignore */
      }
      if (this.selectedProject && this.detailTab) {
        var tab = this.detailTab;
        if (
          tab === 'overview' ||
          tab === 'orchestrator' ||
          tab === 'agents' ||
          tab === 'project_agents' ||
          tab === 'mattermost'
        ) {
          await this.loadDetailTab(tab, true);
        }
      }
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
      if (tab === 'chat') tab = 'agents';
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
        (cur === 'workflow_runs' && this._detailLoaded.workflow_runs) ||
        (cur === 'project_agents' && this._detailLoaded.project_agents) ||
        (cur === 'mattermost' && this._detailLoaded.mattermost) ||
        (cur === 'orchestrator' && this._detailLoaded.overview) ||
        (cur === 'agents' && this._detailLoaded.agents);
      var tabs = ['overview', 'agents', 'backlog', 'board', 'pert', 'docs', 'decisions', 'milestones'];
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
        agents: false,
        backlog: false,
        pert: false,
        spokes: false,
        workflow_runs: false,
        project_agents: false,
        mattermost: false,
        docs: false,
        decisions: false,
        milestones: false,
      };
      this.detailTasks = [];
      this.detailSpokes = [];
      this.detailAgents = [];
      this.detailPipelines = [];
      this.detailConduitRuns = [];
      this.clearConduitRunDetail();
      this.projectAgentsManagement = null;
      this.orchestratorStartSubmitting = false;
      this.detailErrors = {
        overview: '',
        agents: '',
        backlog: '',
        spokes: '',
        workflow_runs: '',
        project_agents: '',
        mattermost: '',
        docs: '',
        decisions: '',
        milestones: '',
        pert: '',
      };
      this.detailLoading = {
        overview: false,
        agents: false,
        backlog: false,
        pert: false,
        spokes: false,
        workflow_runs: false,
        project_agents: false,
        mattermost: false,
        docs: false,
        decisions: false,
        milestones: false,
      };
      this.mattermostForm = { team_name: '', channel_name: '' };
      this.mattermostTestSending = false;
      this.mattermostError = '';
      this._orchestratorAgentMeta = null;
      this.projectChatAgents = [];
      this.projectChatAgentsLoading = false;
      this.projectChatSelectedAgentId = null;
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
      this.clearConduitRunDetail();
      this.taskModalOpen = false;
      this.taskDetailHtml = '';
      this.backlogDetailTask = null;
      if (typeof this.resetBacklogSearch === 'function') this.resetBacklogSearch();
      this.pushProjectsHash();
    },

    retryLoad(tab) {
      if (tab === 'orchestrator') {
        this.setDetailLoaded('overview', false);
      } else {
        this.setDetailLoaded(tab, false);
      }
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
      if (tab === 'orchestrator') {
        await this.loadDetailTab('overview', force, silent);
        return;
      }
      if (tab === 'agents') {
        if (!force && this._detailLoaded.agents) return;
        var hideAg = !!silent;
        if (!hideAg) {
          this.setDetailLoading('agents', true);
          this.setDetailError('agents', '');
        }
        try {
          var pidCh = this.selectedProject.id;
          var rowsCh = await OpenFangAPI.get(
            '/api/projects/' + encodeURIComponent(pidCh) + '/agents'
          );
          this.projectChatAgents = Array.isArray(rowsCh) ? rowsCh : [];
          await this.mergeOrchestratorIntoProjectChatAgents({ skipOrchestratorRow: true });
          var oid = this.orchestratorAgentIdString();
          if (
            oid &&
            this.projectChatSelectedAgentId &&
            String(this.projectChatSelectedAgentId) === oid
          ) {
            this.clearProjectChatAgent();
          }
          if (
            this.projectChatSelectedAgentId &&
            !this.projectChatAgentsExcludingOrchestrator().some(function (r) {
              return String(r.agent_id) === String(this.projectChatSelectedAgentId);
            }, this)
          ) {
            this.projectChatSelectedAgentId = null;
          }
          this.setDetailLoaded('agents', true);
        } catch (e) {
          if (!hideAg) this.setDetailError('agents', e.message || 'Load failed');
          this.projectChatAgents = [];
        }
        if (!hideAg) this.setDetailLoading('agents', false);
        return;
      }
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
            OpenFangAPI.get(base + '/conduits?limit=50'),
          ]);
          this.projectOverview = results[0];
          this.detailSpokes = Array.isArray(results[1]) ? results[1] : [];
          this.detailAgents = Array.isArray(results[2]) ? results[2] : [];
          this.detailPipelines = Array.isArray(results[3]) ? results[3] : [];
          var detailOv = await OpenFangAPI.get(base);
          this.mergeProjectMattermostFieldsFromDetail(detailOv);
          await this.refreshOrchestratorMeta();
          this.setDetailLoaded('overview', true);
          this.setDetailLoaded('spokes', true);
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
      if (tab === 'project_agents') {
        if (!force && this._detailLoaded.project_agents) return;
        var hidePa = !!silent;
        if (!hidePa) {
          this.setDetailLoading('project_agents', true);
          this.setDetailError('project_agents', '');
        }
        try {
          var pidPa = this.selectedProject.id;
          var mgmt = await OpenFangAPI.get(
            '/api/projects/' + encodeURIComponent(pidPa) + '/agents/management'
          );
          this.projectAgentsManagement =
            mgmt && typeof mgmt === 'object'
              ? mgmt
              : { orchestrator_agent_id: null, current: [], historical: [] };
          if (
            this.selectedProject &&
            this.projectAgentsManagement &&
            this.projectAgentsManagement.orchestrator_agent_id != null
          ) {
            this.selectedProject.orchestrator_agent_id =
              this.projectAgentsManagement.orchestrator_agent_id;
          }
          await this.refreshOrchestratorMeta();
          this.setDetailLoaded('project_agents', true);
        } catch (e) {
          if (!hidePa) this.setDetailError('project_agents', e.message || 'Load failed');
          this.projectAgentsManagement = null;
        }
        if (!hidePa) this.setDetailLoading('project_agents', false);
        return;
      }
      if (tab === 'mattermost') {
        var hideMm = !!silent;
        if (!hideMm) {
          this.setDetailLoading('mattermost', true);
          this.setDetailError('mattermost', '');
        }
        try {
          var pidMm = this.selectedProject.id;
          var detailMm = await OpenFangAPI.get(
            '/api/projects/' + encodeURIComponent(pidMm)
          );
          this.mergeProjectMattermostFieldsFromDetail(detailMm);
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
      var hideSpinner = !!silent && (tab === 'backlog' || tab === 'workflow_runs');
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
        } else if (tab === 'workflow_runs') {
          var wr = await OpenFangAPI.get(
            '/api/projects/' + encodeURIComponent(pid) + '/conduit-runs?limit=50'
          );
          this.detailConduitRuns = Array.isArray(wr) ? wr : [];
          if (
            this.conduitRunSelectedId &&
            !this.detailConduitRuns.some(function (r) {
              return String(r.id) === String(this.conduitRunSelectedId);
            }, this)
          ) {
            this.clearConduitRunDetail();
          }
        }
        this.setDetailLoaded(tab, true);
      } catch (e) {
        if (!hideSpinner) this.setDetailError(tab, e.message || 'Load failed');
      }
      if (!hideSpinner) this.setDetailLoading(tab, false);
    },

    async onDetailTabChange(tab) {
      if (tab !== 'workflow_runs') {
        this.clearConduitRunDetail();
      }
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

    /** @returns {Promise<boolean>} */
    async openProjectChatFromMgmtRow(row) {
      var ok = await this.selectProjectChatAgent(row);
      if (ok) await this.onDetailTabChange('agents');
    },

    async selectProjectChatAgent(row) {
      if (!row || row.agent_id == null) return false;
      if (this.isRowProjectOrchestrator(row)) return false;
      var id = String(row.agent_id);
      try {
        var raw = await OpenFangAPI.get('/api/agents/' + encodeURIComponent(id));
        var m = raw.model || {};
        Alpine.store('app').pendingAgent = {
          id: raw.id,
          name: raw.name,
          state: raw.state,
          model_provider: raw.model_provider != null ? raw.model_provider : m.provider || '?',
          model_name: raw.model_name != null ? raw.model_name : m.model || '?',
          identity: raw.identity || {},
          mode: raw.mode,
          profile: raw.profile,
        };
        this.projectChatSelectedAgentId = id;
        return true;
      } catch (e) {
        if (typeof OpenFangToast !== 'undefined') {
          OpenFangToast.error(e.message || 'Could not open agent');
        }
        Alpine.store('app').pendingAgent = null;
        this.projectChatSelectedAgentId = null;
        return false;
      }
    },

    clearProjectChatAgent() {
      OpenFangAPI.wsDisconnect();
      Alpine.store('app').pendingAgent = null;
      this.projectChatSelectedAgentId = null;
    },

    projectHasMattermostChannel() {
      var p = this.selectedProject;
      return (
        p &&
        p.mattermost_channel_id != null &&
        String(p.mattermost_channel_id).trim() !== ''
      );
    },

    orchestratorOverviewStatusText() {
      if (!this.projectHasMattermostChannel()) return 'Setup required';
      if (this.orchestratorIsActive()) return 'Running';
      return 'Not running';
    },

    async openProjectOrchestratorTab() {
      await this.onDetailTabChange('orchestrator');
    },

    /** Overview segment → Agents with no agent selected (orchestrator has its own segment). */
    async openProjectAgentsTab() {
      this.clearProjectChatAgent();
      await this.onDetailTabChange('agents');
    },

    /** Overview agent row → select agent and open Agents tab with embed focused. */
    async openProjectChatWithAgentFromOverview(agent) {
      if (!agent || agent.agent_id == null) return;
      var ok = await this.selectProjectChatAgent(agent);
      if (ok) await this.onDetailTabChange('agents');
    },

    orchestratorAgentIdString() {
      var p = this.selectedProject;
      if (!p || p.orchestrator_agent_id == null) return '';
      return String(p.orchestrator_agent_id).trim();
    },

    isRowProjectOrchestrator(row) {
      if (!row || row.agent_id == null) return false;
      var oid = this.orchestratorAgentIdString();
      return !!oid && String(row.agent_id) === oid;
    },

    detailAgentsExcludingOrchestrator() {
      var self = this;
      return (this.detailAgents || []).filter(function (a) {
        return !self.isRowProjectOrchestrator(a);
      });
    },

    projectChatAgentsExcludingOrchestrator() {
      var self = this;
      return (this.projectChatAgents || []).filter(function (a) {
        return !self.isRowProjectOrchestrator(a);
      });
    },

    async mergeOrchestratorIntoProjectChatAgents(opts) {
      if (opts && opts.skipOrchestratorRow) return;
      var oid =
        this.selectedProject && this.selectedProject.orchestrator_agent_id != null
          ? String(this.selectedProject.orchestrator_agent_id).trim()
          : '';
      if (!oid) return;
      var exists = this.projectChatAgents.some(function (r) {
        return String(r.agent_id) === oid;
      });
      if (exists) return;
      try {
        var d = await OpenFangAPI.get('/api/agents/' + encodeURIComponent(oid));
        if (!d || !d.id) return;
        var row = {
          agent_id: d.id,
          name: d.name || d.id,
          state: d.state,
          spoke_name: null,
          binding: 'orchestrator',
        };
        this.projectChatAgents = this.projectChatAgents.concat([row]);
        this.projectChatAgents.sort(function (a, b) {
          var na = (a.name || '').toLowerCase();
          var nb = (b.name || '').toLowerCase();
          return na.localeCompare(nb);
        });
      } catch (e) {
        /* orchestrator row optional */
      }
    },

    projectDetailSubtabLabel(tab) {
      var labels = {
        overview: 'Overview',
        orchestrator: 'Orchestrator',
        agents: 'Agents',
        backlog: 'List',
        board: 'Board',
        pert: 'PERT',
        milestones: 'Milestones',
        docs: 'Docs',
        decisions: 'Decisions',
        spokes: 'Spokes',
        workflow_runs: 'Conduit runs',
        project_agents: 'Agents',
        mattermost: 'Mattermost',
      };
      return labels[tab] || tab;
    },

    projectDetailSubtabIcon(tab) {
      var icons = {
        overview: 'fa-home',
        orchestrator: 'fa-sitemap',
        agents: 'fa-commenting',
        backlog: 'fa-list',
        board: 'fa-th',
        pert: 'fa-share-alt',
        milestones: 'fa-flag',
        docs: 'fa-file-text-o',
        decisions: 'fa-check-square-o',
        spokes: 'fa-code-fork',
        workflow_runs: 'fa-terminal',
        project_agents: 'fa-users',
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

    clearConduitRunDetail() {
      this.conduitRunSelectedId = null;
      this.conduitRunSelectedConduitId = null;
      this.conduitRunDetail = null;
      this.conduitRunDetailLoading = false;
      this.conduitRunDetailError = '';
      this.conduitRunTraceModalOpen = false;
      this.conduitRunTraceLoading = false;
      this.conduitRunTraceError = '';
      this.conduitRunTraceContent = '';
      this.conduitRunTraceNote = '';
    },

    async selectConduitRunRow(row) {
      if (!row || !row.id || !row.conduit_id) return;
      if (String(this.conduitRunSelectedId) === String(row.id) && this.conduitRunDetail) {
        this.clearConduitRunDetail();
        return;
      }
      this.conduitRunSelectedId = row.id;
      this.conduitRunSelectedConduitId = row.conduit_id;
      this.conduitRunDetail = null;
      this.conduitRunDetailError = '';
      this.conduitRunDetailLoading = true;
      try {
        var path =
          '/api/conduits/' +
          encodeURIComponent(row.conduit_id) +
          '/runs/' +
          encodeURIComponent(row.id);
        this.conduitRunDetail = await OpenFangAPI.get(path);
      } catch (e) {
        this.conduitRunDetailError = e.message || 'Failed to load run detail';
        this.conduitRunDetail = null;
      }
      this.conduitRunDetailLoading = false;
    },

    async reloadConduitRunDetail() {
      var rid = this.conduitRunSelectedId;
      var cid =
        (this.conduitRunDetail && this.conduitRunDetail.conduit_id) ||
        this.conduitRunSelectedConduitId;
      if (!rid || !cid) return;
      this.conduitRunDetailError = '';
      this.conduitRunDetailLoading = true;
      try {
        var path =
          '/api/conduits/' +
          encodeURIComponent(cid) +
          '/runs/' +
          encodeURIComponent(rid);
        this.conduitRunDetail = await OpenFangAPI.get(path);
      } catch (e) {
        this.conduitRunDetailError = e.message || 'Failed to load run detail';
        this.conduitRunDetail = null;
      }
      this.conduitRunDetailLoading = false;
    },

    async copyConduitRunDetailJson() {
      if (!this.conduitRunDetail) return;
      try {
        var t = JSON.stringify(this.conduitRunDetail, null, 2);
        await navigator.clipboard.writeText(t);
        if (typeof OpenFangToast !== 'undefined' && OpenFangToast.success) {
          OpenFangToast.success('Run JSON copied');
        }
      } catch (e) {
        if (typeof OpenFangToast !== 'undefined' && OpenFangToast.error) {
          OpenFangToast.error('Copy failed');
        }
      }
    },

    closeConduitRunTraceModal() {
      this.conduitRunTraceModalOpen = false;
    },

    async loadConduitRunTrace() {
      var pid = this.selectedProject && this.selectedProject.id;
      var rid = this.conduitRunSelectedId;
      if (!pid || !rid) return;
      this.conduitRunTraceLoading = true;
      this.conduitRunTraceError = '';
      this.conduitRunTraceContent = '';
      this.conduitRunTraceNote = '';
      try {
        var path =
          '/api/projects/' +
          encodeURIComponent(pid) +
          '/conduit-runs/' +
          encodeURIComponent(rid) +
          '/trace';
        var data = await OpenFangAPI.get(path);
        this.conduitRunTraceContent = data.content || '';
        this.conduitRunTraceNote = data.note || '';
      } catch (e) {
        this.conduitRunTraceError = e.message || 'Failed to load trace';
      }
      this.conduitRunTraceLoading = false;
    },

    async openConduitRunTraceModal() {
      var pid = this.selectedProject && this.selectedProject.id;
      var rid = this.conduitRunSelectedId;
      if (!pid || !rid) return;
      this.conduitRunTraceModalOpen = true;
      await this.loadConduitRunTrace();
    },

    async reloadConduitRunTrace() {
      if (!this.conduitRunTraceModalOpen) return;
      await this.loadConduitRunTrace();
    },

    async copyConduitRunTrace() {
      var t = this.conduitRunTraceContent || '';
      if (!t.trim()) return;
      try {
        await navigator.clipboard.writeText(t);
      } catch (e) {
        try {
          var ta = document.createElement('textarea');
          ta.value = t;
          ta.style.position = 'fixed';
          ta.style.left = '-9999px';
          document.body.appendChild(ta);
          ta.select();
          document.execCommand('copy');
          document.body.removeChild(ta);
        } catch (e2) {
          /* ignore */
        }
      }
    },

    conduitRunStepTokenSummary(step) {
      if (!step) return '—';
      var a = step.input_tokens != null ? Number(step.input_tokens) : null;
      var b = step.output_tokens != null ? Number(step.output_tokens) : null;
      if (!isFinite(a) && !isFinite(b)) return '—';
      return (isFinite(a) ? a : 0) + ' in / ' + (isFinite(b) ? b : 0) + ' out';
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

    truncatePath(path, maxLen) {
      if (!path) return '';
      var n = maxLen || 64;
      if (path.length <= n) return path;
      return '\u2026' + path.slice(-(n - 1));
    },

    /** List `GET /api/projects` omits Mattermost fields; merge from `GET /api/projects/:id`. */
    mergeProjectMattermostFieldsFromDetail(detail) {
      if (!detail || detail.id == null) return;
      var pid = String(detail.id);
      var patch = {
        mattermost_channel_id: detail.mattermost_channel_id,
        mattermost_team_name: detail.mattermost_team_name,
        mattermost_channel_name: detail.mattermost_channel_name,
        orchestrator_agent_id: detail.orchestrator_agent_id,
        former_agent_ids: detail.former_agent_ids,
        updated_at: detail.updated_at,
      };
      var row = null;
      var i;
      for (i = 0; i < this.projects.length; i++) {
        if (String(this.projects[i].id) === pid) {
          row = this.projects[i];
          Object.assign(row, patch);
          break;
        }
      }
      if (row) {
        this.selectedProject = row;
      } else if (this.selectedProject && String(this.selectedProject.id) === pid) {
        Object.assign(this.selectedProject, patch);
      }
    },

    syncMattermostFormFromProject() {
      var p = this.selectedProject;
      if (!p) {
        this.mattermostForm = { team_name: '', channel_name: '' };
        return;
      }
      this.mattermostForm = {
        team_name:
          p.mattermost_team_name != null ? String(p.mattermost_team_name) : '',
        channel_name:
          p.mattermost_channel_name != null
            ? String(p.mattermost_channel_name)
            : '',
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
      // Not in registry — avoid GET /api/agents/:id (404) for stale project orchestrator ids.
      this._orchestratorAgentMeta = { id: oid, name: '', state: '', missing: true };
    },

    orchestratorIsActive() {
      var m = this._orchestratorAgentMeta;
      if (!m || m.missing) return false;
      return String(m.state || '').toLowerCase().indexOf('running') >= 0;
    },

    orchestratorStartEligible() {
      var p = this.selectedProject;
      if (!p) return false;
      var mm =
        p.mattermost_channel_id != null && String(p.mattermost_channel_id).trim() !== '';
      if (!mm) return false;
      var oid = p.orchestrator_agent_id != null ? String(p.orchestrator_agent_id).trim() : '';
      if (!oid) return true;
      return !this.orchestratorIsActive();
    },

    async startProjectOrchestratorFromAutomation() {
      if (!this.selectedProject || this.orchestratorStartSubmitting) return;
      this.orchestratorStartSubmitting = true;
      try {
        await OpenFangAPI.post(
          '/api/projects/' +
            encodeURIComponent(this.selectedProject.id) +
            '/orchestrator/start',
          {}
        );
        if (typeof OpenFangToast !== 'undefined') {
          OpenFangToast.success('Orchestrator started');
        }
        await this.loadProjects();
        var d = await OpenFangAPI.get(
          '/api/projects/' + encodeURIComponent(this.selectedProject.id)
        );
        this.mergeProjectMattermostFieldsFromDetail(d);
        await this.refreshOrchestratorMeta();
        this.setDetailLoaded('project_agents', false);
        this.setDetailLoaded('overview', false);
        this.setDetailLoaded('agents', false);
        await this.loadDetailTab('project_agents', true);
        if (
          this.detailTab === 'overview' ||
          this.detailTab === 'orchestrator' ||
          this.detailTab === 'agents'
        ) {
          await this.loadDetailTab(this.detailTab, true);
        }
        this.setDetailLoaded('mattermost', false);
      } catch (e) {
        if (typeof OpenFangToast !== 'undefined') {
          OpenFangToast.error(e.message || 'Failed to start orchestrator');
        }
      }
      this.orchestratorStartSubmitting = false;
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
        var tname = (this.mattermostForm.team_name || '').trim();
        var cname = (this.mattermostForm.channel_name || '').trim();
        var body =
          tname || cname
            ? {
                mattermost_team_name: tname ? tname : null,
                mattermost_channel_name: cname ? cname : null,
              }
            : {
                mattermost_channel_id: null,
                mattermost_channel_name: null,
                mattermost_team_name: null,
              };
        var pid = this.selectedProject.id;
        await OpenFangAPI.put('/api/projects/' + encodeURIComponent(pid), body);
        OpenFangToast.success('Mattermost channel saved');
        this.setDetailLoaded('overview', false);
        await this.loadProjects();
        var detail = await OpenFangAPI.get('/api/projects/' + encodeURIComponent(pid));
        this.mergeProjectMattermostFieldsFromDetail(detail);
        this.syncMattermostFormFromProject();
        await this.refreshOrchestratorMeta();
        if (
          this.detailTab === 'overview' ||
          this.detailTab === 'orchestrator' ||
          this.detailTab === 'agents'
        ) {
          await this.loadDetailTab(this.detailTab, true);
        }
      } catch (e) {
        this.mattermostError = e.message || 'Save failed';
      }
      this.mattermostSaving = false;
    },

    async clearMattermostBinding() {
      if (!this.selectedProject || this.mattermostSaving) return;
      this.mattermostForm.team_name = '';
      this.mattermostForm.channel_name = '';
      await this.saveMattermostBinding();
    },

    async sendMattermostTestMessage() {
      if (!this.selectedProject || this.mattermostTestSending) return;
      var p = this.selectedProject;
      var cid = p.mattermost_channel_id;
      if (cid == null || String(cid).trim() === '') {
        this.mattermostError = 'Save a channel binding first.';
        return;
      }
      this.mattermostTestSending = true;
      this.mattermostError = '';
      try {
        var r = await OpenFangAPI.post(
          '/api/projects/' + encodeURIComponent(p.id) + '/mattermost/test-message',
          {}
        );
        OpenFangToast.success(
          r && r.message ? String(r.message) : 'Test message sent'
        );
      } catch (e) {
        this.mattermostError = e.message || 'Send failed';
      }
      this.mattermostTestSending = false;
    },

    /** Mattermost / management CTA → project Orchestrator segment (chat + start). */
    async openOrchestratorAgentDetail() {
      if (!this.selectedProject) return;
      await this.onDetailTabChange('orchestrator');
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

