// Backlog docs browser (project detail) — merged into projectsPage via backlogDocsMixins()
'use strict';

function backlogDocsAugmentPaths(node, parentPath) {
  var base = parentPath || '';
  (node.children || []).forEach(function (c) {
    c._path = base ? base + '/' + c.name : String(c.name);
    backlogDocsAugmentPaths(c, c._path);
  });
}

function backlogDocsMixins() {
  return {
    docTreeRoot: null,
    docsSelectedDoc: null,
    docsDocLoading: false,
    docsDocError: '',
    docEditorModalOpen: false,
    docEditorMode: 'create',
    docEditTargetId: null,
    docEditorForm: { title: '', docType: 'guide', categoryPath: '', content: '' },
    docEditorSaving: false,
    docEditorError: '',
    _docEasymdeInst: null,

    docEditorDestroyEasymde() {
      if (this._docEasymdeInst) {
        openfangEasymdeDestroy(this._docEasymdeInst);
        this._docEasymdeInst = null;
      }
    },

    docEditorSyncEasymdeToForm() {
      if (this._docEasymdeInst && this.docEditorForm) {
        this.docEditorForm.content = this._docEasymdeInst.value();
      }
    },

    docEditorScheduleEasymdeMount() {
      var self = this;
      function run() {
        if (!self.docEditorModalOpen) return;
        self.docEditorDestroyEasymde();
        var el = self.$refs.docEasymdeContent;
        if (!el || el.tagName !== 'TEXTAREA') return;
        var initial = self.docEditorForm && self.docEditorForm.content != null ? self.docEditorForm.content : '';
        self._docEasymdeInst = openfangEasymdeMount(el, '280px', initial);
      }
      function afterStable(fn) {
        requestAnimationFrame(function () {
          requestAnimationFrame(fn);
        });
      }
      if (typeof self.$nextTick === 'function') {
        self.$nextTick(function () {
          afterStable(run);
        });
      } else {
        queueMicrotask(function () {
          afterStable(run);
        });
      }
    },

    resetDocsCache() {
      this.docTreeRoot = null;
      this.docsSelectedDoc = null;
      this.docsDocLoading = false;
      this.docsDocError = '';
      this.docEditorDestroyEasymde();
      this.docEditorModalOpen = false;
      this.docEditorError = '';
    },

    openDocCreateModal() {
      this.docEditorMode = 'create';
      this.docEditTargetId = null;
      this.docEditorForm = {
        title: '',
        docType: 'guide',
        categoryPath: '',
        content: '# Title\n\n',
      };
      this.docEditorError = '';
      this.docEditorModalOpen = true;
    },

    openDocEditModal() {
      var d = this.docsSelectedDoc;
      if (!d || !d.id) return;
      this.docEditorMode = 'edit';
      this.docEditTargetId = d.id;
      this.docEditorForm = {
        title: d.title != null ? String(d.title) : '',
        docType: (d.type != null ? String(d.type) : 'guide') || 'guide',
        categoryPath: d.path != null ? String(d.path) : '',
        content: d.rawContent != null ? String(d.rawContent) : '',
      };
      this.docEditorError = '';
      this.docEditorModalOpen = true;
    },

    closeDocEditorModal() {
      this.docEditorDestroyEasymde();
      this.docEditorModalOpen = false;
      this.docEditorError = '';
      this.docEditTargetId = null;
    },

    confirmDeleteDoc() {
      var self = this;
      var d = this.docsSelectedDoc;
      if (!d || !d.id || !this.selectedProject) return;
      var title = (d.title || d.id || '').toString();
      function doit() {
        var pid = self.selectedProject.id;
        var docId = d.id;
        OpenFangAPI.del(
          '/api/projects/' + encodeURIComponent(pid) + '/backlog/docs/' + encodeURIComponent(docId)
        )
          .then(function () {
            self.docsSelectedDoc = null;
            self.setDetailLoaded('docs', false);
            self.setDetailLoaded('overview', false);
            return self.loadDocsTab(true);
          })
          .then(function () {
            if (typeof self.pushProjectsHash === 'function') self.pushProjectsHash();
            if (typeof OpenFangToast !== 'undefined') OpenFangToast.success('Document deleted');
          })
          .catch(function (e) {
            if (typeof OpenFangToast !== 'undefined') OpenFangToast.error(e.message || 'Delete failed');
          });
      }
      if (typeof OpenFangToast !== 'undefined' && OpenFangToast.confirm) {
        OpenFangToast.confirm(
          'Delete document',
          'Delete "' + title + '"? This removes the file from disk and cannot be undone.',
          doit
        );
      } else if (window.confirm('Delete "' + title + '"? This cannot be undone.')) {
        doit();
      }
    },

    async submitDocEditor() {
      if (!this.selectedProject) return;
      this.docEditorSyncEasymdeToForm();
      var pid = this.selectedProject.id;
      var base = '/api/projects/' + encodeURIComponent(pid) + '/backlog/docs';
      this.docEditorSaving = true;
      this.docEditorError = '';
      try {
        if (this.docEditorMode === 'create') {
          var title = (this.docEditorForm.title || '').trim();
          if (!title) {
            this.docEditorError = 'Title is required';
            this.docEditorSaving = false;
            return;
          }
          var created = await OpenFangAPI.post(base, {
            title: title,
            type: (this.docEditorForm.docType || 'guide').trim(),
            categoryPath: (this.docEditorForm.categoryPath || '').trim(),
            content: this.docEditorForm.content != null ? String(this.docEditorForm.content) : '',
          });
          this.closeDocEditorModal();
          if (typeof this.setDetailLoaded === 'function') {
            this.setDetailLoaded('docs', false);
            this.setDetailLoaded('overview', false);
          }
          await this.loadDocsTab(true);
          if (created.id) await this.docsSelectDoc(created.id);
          if (typeof this.pushProjectsHash === 'function') this.pushProjectsHash();
          if (typeof OpenFangToast !== 'undefined') OpenFangToast.success('Document created');
        } else {
          var id = this.docEditTargetId;
          if (!id) {
            this.docEditorSaving = false;
            return;
          }
          var t = (this.docEditorForm.title || '').trim();
          if (!t) {
            this.docEditorError = 'Title is required';
            this.docEditorSaving = false;
            return;
          }
          var updated = await OpenFangAPI.put(base + '/' + encodeURIComponent(id), {
            title: t,
            content: this.docEditorForm.content != null ? String(this.docEditorForm.content) : '',
          });
          this.closeDocEditorModal();
          this.docsSelectedDoc = updated;
          if (typeof this.setDetailLoaded === 'function') {
            this.setDetailLoaded('docs', false);
            this.setDetailLoaded('overview', false);
          }
          await this.loadDocsTab(true, true);
          if (updated.id) await this.docsSelectDoc(updated.id, true);
          if (typeof this.pushProjectsHash === 'function') this.pushProjectsHash();
          if (typeof OpenFangToast !== 'undefined') OpenFangToast.success('Document saved');
        }
      } catch (e) {
        this.docEditorError = e.message || 'Save failed';
      }
      this.docEditorSaving = false;
    },

    /** Flat list for mobile doc picker (folder/title labels). */
    docSelectOptions() {
      var root = this.docTreeRoot;
      if (!root) return [];
      var out = [];
      function walk(node, folderPath) {
        (node.docs || []).forEach(function (d) {
          if (!d || d.id == null) return;
          var title =
            d.title != null && String(d.title).trim() !== ''
              ? String(d.title)
              : String(d.id);
          var label = folderPath ? folderPath + '/' + title : title;
          out.push({ id: d.id, label: label });
        });
        (node.children || []).forEach(function (c) {
          var fp = folderPath ? folderPath + '/' + String(c.name) : String(c.name);
          walk(c, fp);
        });
      }
      walk(root, '');
      return out;
    },

    /** Flat list for desktop sidebar: document title only, sorted by folder path then title. */
    docSidebarRows() {
      var root = this.docTreeRoot;
      if (!root) return [];
      var rows = [];
      function walk(node, folderPath) {
        (node.docs || []).forEach(function (d) {
          if (!d || d.id == null) return;
          rows.push({ doc: d, folderPath: folderPath || '' });
        });
        (node.children || []).forEach(function (c) {
          var fp = folderPath ? folderPath + '/' + String(c.name) : String(c.name);
          walk(c, fp);
        });
      }
      walk(root, '');
      rows.sort(function (a, b) {
        var pa = a.folderPath || '';
        var pb = b.folderPath || '';
        if (pa !== pb) {
          return pa.localeCompare(pb, undefined, { numeric: true, sensitivity: 'base' });
        }
        var ta = (a.doc.title || a.doc.id || '').toString();
        var tb = (b.doc.title || b.doc.id || '').toString();
        return ta.localeCompare(tb, undefined, { numeric: true, sensitivity: 'base' });
      });
      return rows;
    },

    docsCategoryPathLine() {
      var d = this.docsSelectedDoc;
      if (!d) return '';
      var p = d.path != null ? String(d.path).trim() : '';
      return p || 'docs root';
    },

    docTreeHasAny() {
      var r = this.docTreeRoot;
      if (!r) return false;
      if ((r.docs && r.docs.length) || (r.children && r.children.length)) return true;
      return false;
    },

    async loadDocsTab(force, silent) {
      if (!this.selectedProject) return;
      if (!force && this._detailLoaded.docs) return;
      var pid = this.selectedProject.id;
      var hideSpinner = !!silent;
      var keepDocId =
        hideSpinner && this.docsSelectedDoc && this.docsSelectedDoc.id
          ? this.docsSelectedDoc.id
          : null;
      if (!hideSpinner) {
        this.setDetailLoading('docs', true);
        this.setDetailError('docs', '');
        this.docsSelectedDoc = null;
        this.docsDocError = '';
      }
      try {
        var tree = await OpenFangAPI.get(
          '/api/projects/' + encodeURIComponent(pid) + '/backlog/docs'
        );
        this.docTreeRoot = tree;
        if (tree) backlogDocsAugmentPaths(tree, '');
        this.setDetailLoaded('docs', true);
        if (keepDocId) await this.docsSelectDoc(keepDocId, true);
      } catch (e) {
        if (!hideSpinner) {
          this.setDetailError('docs', e.message || 'Failed to load docs tree');
          this.docTreeRoot = null;
        }
      }
      if (!hideSpinner) this.setDetailLoading('docs', false);
    },

    async docsSelectDoc(docId, silentBody) {
      if (!this.selectedProject || !docId) return;
      var pid = this.selectedProject.id;
      var quiet = !!silentBody;
      if (!quiet) {
        this.docsDocLoading = true;
        this.docsDocError = '';
      }
      try {
        this.docsSelectedDoc = await OpenFangAPI.get(
          '/api/projects/' + encodeURIComponent(pid) + '/backlog/docs/' + encodeURIComponent(docId)
        );
        if (typeof this.pushProjectsHash === 'function') this.pushProjectsHash();
      } catch (e) {
        if (!quiet) {
          this.docsSelectedDoc = null;
          this.docsDocError = e.message || 'Failed to load document';
        }
      }
      if (!quiet) this.docsDocLoading = false;
    },

    docsSelectedBodyHtml() {
      var d = this.docsSelectedDoc;
      if (!d || !d.rawContent) return '';
      return typeof renderMarkdown === 'function' ? renderMarkdown(d.rawContent) : '';
    },

    docsTagsStr(d) {
      if (!d || !Array.isArray(d.tags) || !d.tags.length) return '\u2014';
      return d.tags.join(', ');
    },
  };
}
