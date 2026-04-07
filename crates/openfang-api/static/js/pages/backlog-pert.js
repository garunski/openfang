// PERT-style dependency chart (project detail) — merged into projectsPage via backlogPertMixins()
'use strict';

function backlogPertMixins() {
  return {
    pertLoading: false,
    pertError: '',
    _pertLoaded: false,
    _pertDataTasks: [],
    _pertDataMilestones: [],
    pertLayoutNodes: [],
    pertLayoutEdges: [],
    pertChartWidth: 0,
    pertChartHeight: 0,
    pertSkippedDeps: 0,
    pertEdgeCount: 0,
    pertHasCycle: false,
    pertLegend: [],
    /** Pre-built SVG paths only (no layout library; Alpine x-for inside &lt;svg&gt; is unreliable). */
    pertSvgUnderlay: '',
    /** Gray accent + legend for tasks without a milestone (must match legend swatch). */
    pertNoMilestoneColor: '#8b939e',
    pertFocusTaskId: null,
    _pertEdgeSpecs: [],
    pertFilterStatus: '',
    pertStatusOptions: [],
    pertFilterLabel: '',
    pertLabelOptions: [],
    pertPanning: false,
    _pertPanLast: null,
    _pertPanMouseMove: null,
    _pertPanMouseUp: null,

    resetPertCache() {
      this._pertLoaded = false;
      this._pertDataTasks = [];
      this._pertDataMilestones = [];
      this.pertLayoutNodes = [];
      this.pertLayoutEdges = [];
      this.pertChartWidth = 0;
      this.pertChartHeight = 0;
      this.pertSkippedDeps = 0;
      this.pertEdgeCount = 0;
      this.pertHasCycle = false;
      this.pertLegend = [];
      this.pertSvgUnderlay = '';
      this.pertFocusTaskId = null;
      this._pertEdgeSpecs = [];
      this.pertFilterStatus = '';
      this.pertStatusOptions = [];
      this.pertFilterLabel = '';
      this.pertLabelOptions = [];
      this.pertPanEnd();
      this.pertLoading = false;
      this.pertError = '';
    },

    pertPanEnd() {
      if (this._pertPanMouseMove) {
        window.removeEventListener('mousemove', this._pertPanMouseMove);
        window.removeEventListener('mouseup', this._pertPanMouseUp);
        this._pertPanMouseMove = null;
        this._pertPanMouseUp = null;
      }
      this.pertPanning = false;
      this._pertPanLast = null;
    },

    pertPanPointerDown(e) {
      if (e.button !== 0) return;
      var t = e.target;
      if (t && t.closest && t.closest('.pert-node')) return;
      var wrap = this.$refs.pertScrollWrap;
      if (!wrap) return;
      this.pertPanning = true;
      this._pertPanLast = { x: e.clientX, y: e.clientY };
      var self = this;
      this._pertPanMouseMove = function (ev) {
        if (!self.pertPanning || !self._pertPanLast) return;
        var dx = ev.clientX - self._pertPanLast.x;
        var dy = ev.clientY - self._pertPanLast.y;
        wrap.scrollLeft -= dx;
        wrap.scrollTop -= dy;
        self._pertPanLast = { x: ev.clientX, y: ev.clientY };
      };
      this._pertPanMouseUp = function () {
        self.pertPanEnd();
      };
      window.addEventListener('mousemove', this._pertPanMouseMove);
      window.addEventListener('mouseup', this._pertPanMouseUp);
      e.preventDefault();
    },

    _pertRefreshStatusOptions(cfg) {
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
      var st = cfg && cfg.statuses && cfg.statuses.length ? cfg.statuses : [];
      for (i = 0; i < st.length; i++) add(st[i]);
      var all = this._pertDataTasks || [];
      for (i = 0; i < all.length; i++) add(all[i].status);
      this.pertStatusOptions = out;
    },

    _pertRefreshLabelOptions() {
      var seen = {};
      var out = [];
      var all = this._pertDataTasks || [];
      var i;
      var j;
      for (i = 0; i < all.length; i++) {
        var lbs = all[i].labels;
        if (!Array.isArray(lbs)) continue;
        for (j = 0; j < lbs.length; j++) {
          var L = lbs[j] != null ? String(lbs[j]).trim() : '';
          if (!L) continue;
          var k = L.toLowerCase();
          if (seen[k]) continue;
          seen[k] = true;
          out.push(L);
        }
      }
      out.sort(function (a, b) {
        return String(a).localeCompare(String(b), undefined, { sensitivity: 'base' });
      });
      this.pertLabelOptions = out;
    },

    pertMilestoneHue(id) {
      var s = id != null ? String(id).trim() : '';
      if (!s) return 210;
      var h = 0;
      var i;
      for (i = 0; i < s.length; i++) h = ((h * 31 + s.charCodeAt(i)) >>> 0) % 360;
      return h;
    },

    /** CSS color for node border-left + legend (milestone tasks only). */
    pertMilestoneAccentColor(ms) {
      var s = ms != null ? String(ms).trim() : '';
      if (!s) return this.pertNoMilestoneColor;
      var hue = this.pertMilestoneHue(s);
      return 'hsl(' + hue + ',52%,46%)';
    },

    pertSetChartFocus(id) {
      this.pertFocusTaskId = id || null;
      this._pertPaintEdgeSvg();
    },

    _pertPaintEdgeSvg() {
      var specs = this._pertEdgeSpecs || [];
      var focus = this.pertFocusTaskId;
      var defs =
        '<defs><marker id="pertArrowHead" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="7" markerHeight="7" orient="auto"><path d="M 0 0 L 10 5 L 0 10 z" fill="#64748b"/></marker></defs>';
      var parts = [defs];
      var i;
      for (i = 0; i < specs.length; i++) {
        var s = specs[i];
        var hi = focus && (s.from === focus || s.to === focus);
        var lo = focus && !hi;
        var op = focus ? (hi ? 1 : 0.07) : 0.72;
        var sw = hi ? 2.4 : lo ? 1 : 1.65;
        var stroke = hi ? '#475569' : '#64748b';
        parts.push(
          '<path d="' +
            s.d +
            '" fill="none" stroke="' +
            stroke +
            '" stroke-width="' +
            sw +
            '" opacity="' +
            op +
            '" marker-end="url(#pertArrowHead)"/>'
        );
      }
      this.pertSvgUnderlay = parts.join('');
    },

    _pertMilestoneTitleMap() {
      var m = {};
      var list = this._pertDataMilestones || [];
      for (var i = 0; i < list.length; i++) {
        var row = list[i];
        if (row && row.id) m[String(row.id)] = row.title || row.id;
      }
      return m;
    },

    _pertTaskMilestoneId(t) {
      var x = t.milestone != null ? String(t.milestone).trim() : '';
      return x || '';
    },

    /** Resolve task id for edges (case-insensitive). */
    _pertCanonicalId(idSetLower, raw) {
      var s = raw != null ? String(raw).trim() : '';
      if (!s) return '';
      var canon = idSetLower[s.toLowerCase()];
      return canon || '';
    },

    _pertTaskDepsList(t) {
      var out = [];
      var seen = {};
      var push = function (x) {
        var v = x != null ? String(x).trim() : '';
        if (!v || seen[v]) return;
        seen[v] = true;
        out.push(v);
      };
      var i;
      var arr = t.dependencies;
      if (Array.isArray(arr)) {
        for (i = 0; i < arr.length; i++) push(arr[i]);
      }
      if (typeof t.dependencies === 'string' && t.dependencies.trim()) {
        t.dependencies
          .split(/[\s,]+/)
          .forEach(function (p) {
            push(p);
          });
      }
      var par = t.parentTaskId != null ? t.parentTaskId : t.parent_task_id;
      if (par) push(par);
      return out;
    },

    rebuildPertLayout() {
      this.pertFocusTaskId = null;
      this.pertPanEnd();

      var NODE_W = 184;
      var NODE_H = 68;
      var GAP_X = 56;
      var GAP_Y = 20;
      var PAD = 20;
      /** Top band for dependency “bus” so edges skipping columns don’t cut through intermediate nodes */
      var EDGE_LANE_H = 24;
      var pertBusY = PAD + 8;

      var allTasks = Array.isArray(this._pertDataTasks) ? this._pertDataTasks.slice() : [];
      var fs = String(this.pertFilterStatus || '').trim();
      var tasks;
      if (fs) {
        var fl = fs.toLowerCase();
        tasks = allTasks.filter(function (t) {
          return String(t.status || '').toLowerCase() === fl;
        });
      } else {
        tasks = allTasks;
      }
      var flb = String(this.pertFilterLabel || '').trim();
      if (flb) {
        var flbl = flb.toLowerCase();
        tasks = tasks.filter(function (t) {
          var lbs = t.labels;
          if (!Array.isArray(lbs) || !lbs.length) return false;
          var k;
          for (k = 0; k < lbs.length; k++) {
            if (String(lbs[k] || '').trim().toLowerCase() === flbl) return true;
          }
          return false;
        });
      }
      var idSetLower = {};
      var i;
      var j;
      for (i = 0; i < tasks.length; i++) {
        if (tasks[i] && tasks[i].id) {
          var cid = String(tasks[i].id);
          idSetLower[cid.toLowerCase()] = cid;
        }
      }

      var preds = {};
      var skipped = 0;
      var rawEdges = [];

      for (i = 0; i < tasks.length; i++) {
        var t = tasks[i];
        var tid = t && t.id ? String(t.id) : '';
        if (!tid) continue;
        preds[tid] = preds[tid] || [];
        var deps = this._pertTaskDepsList(t);
        for (j = 0; j < deps.length; j++) {
          var dRaw = deps[j];
          var d = this._pertCanonicalId(idSetLower, dRaw);
          if (!d) {
            skipped++;
            continue;
          }
          rawEdges.push({ from: d, to: tid });
          if (preds[tid].indexOf(d) === -1) preds[tid].push(d);
        }
      }

      this.pertSkippedDeps = skipped;
      this.pertEdgeCount = rawEdges.length;

      var indeg = {};
      var nodes = [];
      var nk;
      for (nk in idSetLower) {
        if (Object.prototype.hasOwnProperty.call(idSetLower, nk)) nodes.push(idSetLower[nk]);
      }
      for (i = 0; i < nodes.length; i++) indeg[nodes[i]] = 0;
      for (i = 0; i < rawEdges.length; i++) {
        indeg[rawEdges[i].to] = (indeg[rawEdges[i].to] || 0) + 1;
      }
      var q = [];
      for (i = 0; i < nodes.length; i++) {
        if (indeg[nodes[i]] === 0) q.push(nodes[i]);
      }
      var topo = [];
      var qi = 0;
      while (qi < q.length) {
        var u = q[qi++];
        topo.push(u);
        for (i = 0; i < rawEdges.length; i++) {
          var e = rawEdges[i];
          if (e.from !== u) continue;
          indeg[e.to]--;
          if (indeg[e.to] === 0) q.push(e.to);
        }
      }
      this.pertHasCycle = topo.length < nodes.length;

      var level = {};
      var iter;
      var changed;
      for (iter = 0; iter < nodes.length + 3; iter++) {
        changed = false;
        for (i = 0; i < tasks.length; i++) {
          var id = tasks[i].id ? String(tasks[i].id) : '';
          if (!id) continue;
          var ps = preds[id] || [];
          if (ps.length === 0) {
            if (level[id] !== 0) {
              level[id] = 0;
              changed = true;
            }
          } else {
            var m = -1;
            for (j = 0; j < ps.length; j++) {
              var lp = level[ps[j]];
              if (lp !== undefined) m = Math.max(m, lp);
            }
            if (m >= 0) {
              var nl = m + 1;
              if (level[id] !== nl) {
                level[id] = nl;
                changed = true;
              }
            }
          }
        }
        if (!changed) break;
      }

      var maxL = 0;
      for (i = 0; i < tasks.length; i++) {
        var lid = tasks[i].id ? String(tasks[i].id) : '';
        if (lid && level[lid] != null) maxL = Math.max(maxL, level[lid]);
      }
      var orphanLevel = maxL + 1;
      for (i = 0; i < tasks.length; i++) {
        var oid = tasks[i].id ? String(tasks[i].id) : '';
        if (oid && level[oid] == null) {
          level[oid] = orphanLevel;
          orphanLevel++;
        }
      }

      var msTitles = this._pertMilestoneTitleMap();
      var byLevel = {};
      for (i = 0; i < tasks.length; i++) {
        var tk = tasks[i];
        var tkid = tk.id ? String(tk.id) : '';
        if (!tkid) continue;
        var lv = level[tkid];
        if (lv == null) lv = 0;
        if (!byLevel[lv]) byLevel[lv] = [];
        byLevel[lv].push(tk);
      }
      var levels = Object.keys(byLevel)
        .map(function (k) {
          return parseInt(k, 10);
        })
        .sort(function (a, b) {
          return a - b;
        });

      for (i = 0; i < levels.length; i++) {
        var L = levels[i];
        byLevel[L].sort(function (a, b) {
          var ma = this._pertTaskMilestoneId(a);
          var mb = this._pertTaskMilestoneId(b);
          if (ma !== mb) return ma < mb ? -1 : ma > mb ? 1 : 0;
          var ia = String(a.id || '');
          var ib = String(b.id || '');
          return ia < ib ? -1 : ia > ib ? 1 : 0;
        }.bind(this));
      }

      var pos = {};
      for (i = 0; i < levels.length; i++) {
        var col = levels[i];
        var rowTasks = byLevel[col];
        for (j = 0; j < rowTasks.length; j++) {
          var rid = rowTasks[j].id ? String(rowTasks[j].id) : '';
          if (!rid) continue;
          pos[rid] = {
            x: PAD + col * (NODE_W + GAP_X),
            y: PAD + EDGE_LANE_H + j * (NODE_H + GAP_Y),
            col: col,
            row: j,
          };
        }
      }

      var layoutNodes = [];
      for (i = 0; i < tasks.length; i++) {
        var tt = tasks[i];
        var ttid = tt.id ? String(tt.id) : '';
        if (!ttid || !pos[ttid]) continue;
        var p = pos[ttid];
        var ms = this._pertTaskMilestoneId(tt);
        layoutNodes.push({
          id: ttid,
          task: tt,
          x: p.x,
          y: p.y,
          w: NODE_W,
          h: NODE_H,
          milestoneId: ms,
          accentColor: this.pertMilestoneAccentColor(ms),
        });
      }

      var layoutEdges = [];
      this._pertEdgeSpecs = [];
      var edgeWork = [];
      var ARROW_GAP = 7;
      for (i = 0; i < rawEdges.length; i++) {
        var ed0 = rawEdges[i];
        var af = pos[ed0.from];
        var bt = pos[ed0.to];
        if (!af || !bt) continue;
        var x1 = af.x + NODE_W;
        var y1 = af.y + NODE_H / 2;
        var x2 = bt.x;
        var y2 = bt.y + NODE_H / 2;
        var c1 = af.col != null ? af.col : 0;
        var c2 = bt.col != null ? bt.col : 0;
        edgeWork.push({
          from: ed0.from,
          to: ed0.to,
          x1: x1,
          y1: y1,
          x2: x2,
          y2: y2,
          c1: c1,
          c2: c2,
          midY: (y1 + y2) / 2,
        });
      }

      var gk;
      var groups = {};
      for (i = 0; i < edgeWork.length; i++) {
        var ew = edgeWork[i];
        gk = ew.c1 + ':' + ew.c2;
        if (!groups[gk]) groups[gk] = [];
        groups[gk].push(ew);
      }
      for (gk in groups) {
        if (!Object.prototype.hasOwnProperty.call(groups, gk)) continue;
        var arr = groups[gk];
        arr.sort(function (u, v) {
          return u.midY - v.midY;
        });
        var n = arr.length;
        var gutter = arr[0].x2 - arr[0].x1;
        var step = n > 1 ? Math.min(14, Math.max(6, (gutter - 16) / n)) : 0;
        var baseMid = (arr[0].x1 + arr[0].x2) / 2;
        var laneMax = PAD + EDGE_LANE_H - 4;
        for (j = 0; j < n; j++) {
          var ej = arr[j];
          var off = n === 1 ? 0 : (j - (n - 1) / 2) * step;
          var midX = baseMid + off;
          var minM = ej.x1 + 8;
          var maxM = ej.x2 - 8;
          if (maxM > minM) midX = Math.max(minM, Math.min(maxM, midX));
          var endX = ej.x2 - ARROW_GAP;
          if (endX <= ej.x1 + 2) endX = ej.x2 - 2;
          var dMan;
          if (ej.c2 > ej.c1 + 1) {
            var busY = Math.min(laneMax, pertBusY + j * 5);
            var xL = ej.x1 + GAP_X / 2;
            var xR = ej.x2 - GAP_X / 2;
            dMan =
              'M ' +
              ej.x1 +
              ' ' +
              ej.y1 +
              ' L ' +
              xL +
              ' ' +
              ej.y1 +
              ' L ' +
              xL +
              ' ' +
              busY +
              ' L ' +
              xR +
              ' ' +
              busY +
              ' L ' +
              xR +
              ' ' +
              ej.y2 +
              ' L ' +
              endX +
              ' ' +
              ej.y2;
          } else {
            dMan =
              'M ' +
              ej.x1 +
              ' ' +
              ej.y1 +
              ' L ' +
              midX +
              ' ' +
              ej.y1 +
              ' L ' +
              midX +
              ' ' +
              ej.y2 +
              ' L ' +
              endX +
              ' ' +
              ej.y2;
          }
          layoutEdges.push({ key: ej.from + '->' + ej.to, d: dMan });
          this._pertEdgeSpecs.push({ from: ej.from, to: ej.to, d: dMan });
        }
      }

      var chartW = PAD * 2 + NODE_W;
      var chartH = PAD * 2 + NODE_H;
      for (i = 0; i < layoutNodes.length; i++) {
        var ln = layoutNodes[i];
        chartW = Math.max(chartW, ln.x + ln.w + PAD);
        chartH = Math.max(chartH, ln.y + ln.h + PAD);
      }

      this.pertLayoutNodes = layoutNodes;
      this.pertLayoutEdges = layoutEdges;
      this.pertChartWidth = layoutNodes.length ? chartW : PAD * 2 + NODE_W;
      this.pertChartHeight = layoutNodes.length ? chartH : PAD * 2 + NODE_H;

      this._pertPaintEdgeSvg();

      var legendMap = {};
      for (i = 0; i < tasks.length; i++) {
        var tm = this._pertTaskMilestoneId(tasks[i]);
        var key = tm || '__none__';
        if (!legendMap[key]) {
          legendMap[key] = {
            id: key,
            title:
              key === '__none__'
                ? 'No milestone'
                : msTitles[tm] || tm || 'Unknown',
            swatchStyle:
              key === '__none__'
                ? 'background:' + this.pertNoMilestoneColor
                : 'background:' + this.pertMilestoneAccentColor(tm),
            count: 0,
          };
        }
        legendMap[key].count++;
      }
      var leg = Object.keys(legendMap)
        .map(function (k) {
          return legendMap[k];
        })
        .sort(function (a, b) {
          if (a.id === '__none__') return 1;
          if (b.id === '__none__') return -1;
          return String(a.title).localeCompare(String(b.title));
        });
      this.pertLegend = leg;
    },

    pertNodeTitle(n) {
      var t = n && n.task && n.task.title ? String(n.task.title) : '';
      if (t.length > 72) return t.slice(0, 70) + '\u2026';
      return t || '\u2014';
    },

    /** Aligns with statusClass() / list view: terminal backlog states */
    pertTaskIsCompleted(t) {
      if (!t || t.status == null || t.status === '') return false;
      var x = String(t.status).toLowerCase().trim();
      return x === 'done' || x === 'closed' || x === 'complete' || x === 'completed';
    },

    async loadPertTab(force, silent) {
      if (!this.selectedProject) return;
      if (!force && this._pertLoaded) return;
      var pid = this.selectedProject.id;
      var hideSpinner =
        !!silent && (this._pertDataTasks.length > 0 || this._pertDataMilestones.length > 0);
      if (!hideSpinner) {
        this.pertLoading = true;
        this.pertError = '';
      }
      try {
        var base = '/api/projects/' + encodeURIComponent(pid);
        var triple = await Promise.all([
          OpenFangAPI.get(base + '/backlog/tasks'),
          OpenFangAPI.get(base + '/backlog/milestones'),
          OpenFangAPI.get(base + '/backlog/config'),
        ]);
        this._pertDataTasks = Array.isArray(triple[0]) ? triple[0] : [];
        this._pertDataMilestones = Array.isArray(triple[1]) ? triple[1] : [];
        this._pertRefreshStatusOptions(triple[2]);
        this._pertRefreshLabelOptions();
        this.rebuildPertLayout();
        this._pertLoaded = true;
      } catch (e) {
        if (!hideSpinner) {
          this.pertError = e.message || 'Failed to load PERT data';
          this._pertDataTasks = [];
          this._pertDataMilestones = [];
          this._pertRefreshStatusOptions(null);
          this._pertRefreshLabelOptions();
          this.rebuildPertLayout();
        }
      }
      if (!hideSpinner) this.pertLoading = false;
    },
  };
}
