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

    resetDocsCache() {
      this.docTreeRoot = null;
      this.docExpandedKeys = {};
      this.docsSelectedDoc = null;
      this.docsDocLoading = false;
      this.docsDocError = '';
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

    async loadDocsTab(force) {
      if (!this.selectedProject) return;
      if (!force && this._detailLoaded.docs) return;
      var pid = this.selectedProject.id;
      this.setDetailLoading('docs', true);
      this.setDetailError('docs', '');
      this.docsSelectedDoc = null;
      this.docsDocError = '';
      try {
        var tree = await OpenFangAPI.get(
          '/api/projects/' + encodeURIComponent(pid) + '/backlog/docs'
        );
        this.docTreeRoot = tree;
        if (tree) backlogDocsAugmentPaths(tree, '');
        this.setDetailLoaded('docs', true);
      } catch (e) {
        this.setDetailError('docs', e.message || 'Failed to load docs tree');
        this.docTreeRoot = null;
      }
      this.setDetailLoading('docs', false);
    },

    async docsSelectDoc(docId) {
      if (!this.selectedProject || !docId) return;
      var pid = this.selectedProject.id;
      this.docsDocLoading = true;
      this.docsDocError = '';
      try {
        this.docsSelectedDoc = await OpenFangAPI.get(
          '/api/projects/' + encodeURIComponent(pid) + '/backlog/docs/' + encodeURIComponent(docId)
        );
      } catch (e) {
        this.docsSelectedDoc = null;
        this.docsDocError = e.message || 'Failed to load document';
      }
      this.docsDocLoading = false;
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
