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
    docExpandedKeys: {},
    docsSelectedDoc: null,
    docsDocLoading: false,
    docsDocError: '',
    docEditorModalOpen: false,
    docEditorMode: 'create',
    docEditTargetId: null,
    docEditorForm: { title: '', docType: 'guide', categoryPath: '', content: '' },
    docEditorSaving: false,
    docEditorError: '',

    resetDocsCache() {
      this.docTreeRoot = null;
      this.docExpandedKeys = {};
      this.docsSelectedDoc = null;
      this.docsDocLoading = false;
      this.docsDocError = '';
      this.docEditorModalOpen = false;
      this.docEditorError = '';
    },

    expandDocPathFolders(path) {
      if (!path || typeof path !== 'string') return;
      var parts = path.split('/').filter(Boolean);
      var acc = '';
      var o = Object.assign({}, this.docExpandedKeys);
      var i;
      for (i = 0; i < parts.length; i++) {
        acc = acc ? acc + '/' + parts[i] : parts[i];
        o[acc] = true;
      }
      this.docExpandedKeys = o;
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
          this.docEditorModalOpen = false;
          this.expandDocPathFolders(created.path);
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
          this.docEditorModalOpen = false;
          this.expandDocPathFolders(updated.path);
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

    docNavRows() {
      var root = this.docTreeRoot;
      if (!root) return [];
      var exp = this.docExpandedKeys;
      var rows = [];
      function walk(node, depth) {
        if (depth === 0) {
          (node.docs || []).forEach(function (d) {
            rows.push({ kind: 'doc', doc: d, depth: 0 });
          });
        }
        (node.children || []).forEach(function (c) {
          rows.push({ kind: 'folder', name: c.name, path: c._path, depth: depth });
          if (exp[c._path]) {
            (c.docs || []).forEach(function (d) {
              rows.push({ kind: 'doc', doc: d, depth: depth + 1 });
            });
            walk(c, depth + 1);
          }
        });
      }
      walk(root, 0);
      return rows;
    },

    docTreeHasAny() {
      var r = this.docTreeRoot;
      if (!r) return false;
      if ((r.docs && r.docs.length) || (r.children && r.children.length)) return true;
      return false;
    },

    docToggleFolder(path) {
      var o = Object.assign({}, this.docExpandedKeys);
      o[path] = !o[path];
      this.docExpandedKeys = o;
    },

    docFolderGlyph(path) {
      return this.docFolderExpanded(path) ? '\u25bc' : '\u25b6';
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
