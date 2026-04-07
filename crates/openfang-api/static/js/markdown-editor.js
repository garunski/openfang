// Shared EasyMDE defaults for dashboard modals (depends on app.js renderMarkdownPreview).
'use strict';

function openfangEasymdeBaseOpts(minHeight) {
  return {
    autoDownloadFontAwesome: false,
    spellChecker: false,
    status: false,
    sideBySideFullscreen: false,
    minHeight: minHeight || '200px',
    /* Bulma minireset removes list bullets unless .markdown-body applies GFM list rules */
    previewClass: ['editor-preview', 'markdown-body'],
    renderingConfig: {
      codeSyntaxHighlighting: typeof hljs !== 'undefined',
    },
    previewRender: function (plainText) {
      return typeof renderMarkdownPreview === 'function'
        ? renderMarkdownPreview(plainText)
        : typeof renderMarkdown === 'function'
          ? renderMarkdown(plainText)
          : String(plainText || '');
    },
    toolbar: [
      'bold',
      'italic',
      'strikethrough',
      '|',
      'heading',
      'heading-smaller',
      '|',
      'horizontal-rule',
      '|',
      'quote',
      'unordered-list',
      'ordered-list',
      '|',
      'link',
      'code',
      '|',
      'preview',
      'side-by-side',
    ],
  };
}

function openfangEasymdeMount(textareaEl, minHeight, initialValue) {
  if (!textareaEl || textareaEl.tagName !== 'TEXTAREA' || typeof EasyMDE === 'undefined') {
    return null;
  }
  textareaEl.value = initialValue != null ? String(initialValue) : '';
  try {
    return new EasyMDE(Object.assign({ element: textareaEl }, openfangEasymdeBaseOpts(minHeight)));
  } catch (e) {
    console.warn('[OpenFang] EasyMDE mount failed', e);
    return null;
  }
}

function openfangEasymdeDestroy(inst) {
  if (!inst) return;
  try {
    inst.toTextArea();
  } catch (e1) {}
}
