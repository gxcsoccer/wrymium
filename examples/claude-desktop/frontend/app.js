// Claude Desktop — Frontend Application
// Two-column layout with sidebar conversation management
// Communicates with Rust backend via window.ipc.postMessage()

(function () {
  'use strict';

  // --- State ---
  const state = {
    conversations: [],
    activeConversationId: null,
    messages: [],
    isStreaming: false,
    isReady: false,
    activeMode: 'chat',
    modelName: 'Claude',
    currentAssistantEl: null,
    currentTextContent: '',
    toolCounter: 0,
    userScrolledUp: false,
    lastToolUse: null,
    artifacts: [],
    activeArtifactId: null,
    fileCwd: '',
    fileTreeLoaded: false,
    browseActive: false,
  };

  // --- DOM refs ---
  const appEl = document.getElementById('app');
  const sidebarEl = document.getElementById('sidebar');
  const messagesEl = document.getElementById('messages');
  const welcomeEl = document.getElementById('welcome');
  const inputEl = document.getElementById('input');
  const sendBtn = document.getElementById('send-btn');
  const stopBtn = document.getElementById('stop-btn');
  const thinkingEl = document.getElementById('thinking');
  const convListEl = document.getElementById('conversation-list');
  const convTitleEl = document.getElementById('conversation-title');
  const modelDisplayEl = document.getElementById('model-display');
  const searchInput = document.getElementById('search-input');
  const sidebarCollapseBtn = document.getElementById('sidebar-collapse-btn');
  const sidebarExpandBtn = document.getElementById('sidebar-expand-btn');

  // --- Markdown setup ---
  if (typeof marked !== 'undefined') {
    marked.setOptions({
      breaks: true,
      gfm: true,
      highlight: function (code, lang) {
        if (typeof hljs !== 'undefined' && lang && hljs.getLanguage(lang)) {
          try { return hljs.highlight(code, { language: lang }).value; } catch (e) {}
        }
        if (typeof hljs !== 'undefined') {
          try { return hljs.highlightAuto(code).value; } catch (e) {}
        }
        return code;
      },
    });
  }

  // --- IPC ---
  function send(cmd, data) {
    if (window.ipc && window.ipc.postMessage) {
      window.ipc.postMessage(JSON.stringify({ cmd, ...data }));
    }
  }

  // --- Sidebar ---

  sidebarCollapseBtn.addEventListener('click', function () {
    state.sidebarCollapsed = true;
    appEl.classList.add('sidebar-collapsed');
    sidebarExpandBtn.classList.remove('hidden');
  });

  sidebarExpandBtn.addEventListener('click', function () {
    state.sidebarCollapsed = false;
    appEl.classList.remove('sidebar-collapsed');
    sidebarExpandBtn.classList.add('hidden');
  });

  document.getElementById('sidebar-new-chat').addEventListener('click', startNewChat);

  searchInput.addEventListener('input', function () {
    renderConversationList(this.value.trim().toLowerCase());
  });

  function renderConversationList(filter) {
    convListEl.innerHTML = '';
    const list = filter
      ? state.conversations.filter(function (c) { return c.title.toLowerCase().includes(filter); })
      : state.conversations;

    for (var i = 0; i < list.length; i++) {
      var conv = list[i];
      var item = document.createElement('div');
      item.className = 'conversation-item' + (conv.id === state.activeConversationId ? ' active' : '');
      item.dataset.id = conv.id;
      item.innerHTML =
        '<span class="conv-title">' + escapeHtml(conv.title) + '</span>' +
        '<span class="conv-time">' + formatRelativeTime(conv.updated_at) + '</span>' +
        '<button class="conv-delete" title="Delete">&times;</button>';
      item.addEventListener('click', (function (id) {
        return function (e) {
          if (e.target.classList.contains('conv-delete')) return;
          switchConversation(id);
        };
      })(conv.id));
      item.querySelector('.conv-delete').addEventListener('click', (function (id) {
        return function (e) {
          e.stopPropagation();
          deleteConversation(id);
        };
      })(conv.id));
      convListEl.appendChild(item);
    }
  }

  function switchConversation(id) {
    if (id === state.activeConversationId) return;
    if (state.isStreaming) {
      send('stop');
      setStreaming(false);
    }
    if (state.activeConversationId && state.messages.length > 0) {
      saveCurrentConversation();
    }
    send('load_conversation', { id: id });
  }

  function saveCurrentConversation() {
    if (!state.activeConversationId) return;
    send('save_conversation', {
      id: state.activeConversationId,
      title: convTitleEl.textContent || 'New conversation',
      messages: state.messages,
    });
  }

  function deleteConversation(id) {
    send('delete_conversation', { id: id });
  }

  function startNewChat() {
    if (state.isStreaming) {
      send('stop');
    }
    if (state.activeConversationId && state.messages.length > 0) {
      saveCurrentConversation();
    }
    state.activeConversationId = null;
    state.messages = [];
    state.currentAssistantEl = null;
    state.currentTextContent = '';
    state.toolCounter = 0;
    convTitleEl.textContent = 'New conversation';
    messagesEl.innerHTML = '';
    messagesEl.appendChild(welcomeEl);
    welcomeEl.classList.remove('hidden');
    setStreaming(false);
    send('new_chat');
    send('list_conversations');
    inputEl.focus();
  }

  // --- IPC Response Handlers ---

  window.__onConversations = function (list) {
    state.conversations = list;
    renderConversationList();
  };

  window.__onConversationLoaded = function (conv) {
    state.activeConversationId = conv.id;
    state.messages = conv.messages || [];
    convTitleEl.textContent = conv.title || 'Untitled';
    renderLoadedMessages();
    renderConversationList();
    send('new_chat'); // respawn CLI for fresh context
  };

  window.__onSaved = function () {
    send('list_conversations');
  };

  window.__onDeleted = function (data) {
    if (state.activeConversationId === data.id) {
      state.activeConversationId = null;
      state.messages = [];
      convTitleEl.textContent = 'New conversation';
      messagesEl.innerHTML = '';
      messagesEl.appendChild(welcomeEl);
      welcomeEl.classList.remove('hidden');
    }
    send('list_conversations');
  };

  function renderLoadedMessages() {
    messagesEl.innerHTML = '';
    welcomeEl.classList.add('hidden');
    for (var i = 0; i < state.messages.length; i++) {
      var msg = state.messages[i];
      if (msg.role === 'user') {
        appendUserMessageEl(msg.content);
      } else if (msg.role === 'assistant') {
        appendAssistantMessageEl(msg.content);
      }
    }
    messagesEl.scrollTop = messagesEl.scrollHeight;
  }

  function appendAssistantMessageEl(text) {
    var el = document.createElement('div');
    el.className = 'message message-assistant';
    el.innerHTML = '<div class="message-content"><div class="assistant-text">' + renderMarkdown(text) + '</div></div>';
    messagesEl.appendChild(el);
  }

  // --- Mode Tabs ---
  document.querySelectorAll('.mode-tab').forEach(function (tab) {
    tab.addEventListener('click', function () {
      var mode = this.dataset.mode;
      state.activeMode = mode;
      document.querySelectorAll('.mode-tab').forEach(function (t) { t.classList.toggle('active', t.dataset.mode === mode); });
      document.querySelectorAll('.mode-panel').forEach(function (p) {
        p.classList.toggle('hidden', p.id !== mode + '-panel');
        if (p.id === mode + '-panel') p.classList.add('active');
        else p.classList.remove('active');
      });
      // Browse WebView lifecycle
      if (mode === 'browse' && !state.browseActive) {
        send('activate_browse');
        state.browseActive = true;
      } else if (mode !== 'browse' && state.browseActive) {
        send('deactivate_browse');
      }
    });
  });

  // --- Scroll management ---
  messagesEl.addEventListener('scroll', function () {
    var atBottom = messagesEl.scrollHeight - messagesEl.scrollTop - messagesEl.clientHeight < 80;
    state.userScrolledUp = !atBottom;
  });

  function autoScroll() {
    if (!state.userScrolledUp) {
      messagesEl.scrollTop = messagesEl.scrollHeight;
    }
  }

  // --- Input ---
  inputEl.addEventListener('input', function () {
    this.style.height = 'auto';
    this.style.height = Math.min(this.scrollHeight, 200) + 'px';
  });

  inputEl.addEventListener('keydown', function (e) {
    if (e.key === 'Enter' && (e.ctrlKey || e.metaKey)) {
      e.preventDefault();
      sendMessage();
    }
  });

  sendBtn.addEventListener('click', sendMessage);
  stopBtn.addEventListener('click', function () { send('stop'); });

  function sendMessage() {
    var text = inputEl.value.trim();
    if (!text || state.isStreaming) return;

    // Create conversation if needed
    if (!state.activeConversationId) {
      state.activeConversationId = '' + Date.now();
      var title = text.length > 50 ? text.substring(0, 50) + '...' : text;
      convTitleEl.textContent = title;
    }

    welcomeEl.classList.add('hidden');
    state.messages.push({ role: 'user', content: text, timestamp: new Date().toISOString() });
    appendUserMessageEl(text);
    send('send', { text: text });

    inputEl.value = '';
    inputEl.style.height = 'auto';
    setStreaming(true);
  }

  function setStreaming(streaming) {
    state.isStreaming = streaming;
    sendBtn.disabled = streaming;
    stopBtn.classList.toggle('hidden', !streaming);
    thinkingEl.classList.toggle('hidden', !streaming);
    if (!streaming) {
      state.currentAssistantEl = null;
      state.currentTextContent = '';
      state.toolCounter = 0;
    }
  }

  // --- Message rendering ---
  function appendUserMessageEl(text) {
    var el = document.createElement('div');
    el.className = 'message message-user';
    el.innerHTML = '<div class="message-content">' + escapeHtml(text) + '</div>';
    messagesEl.appendChild(el);
    autoScroll();
  }

  function getOrCreateAssistantMessage() {
    if (!state.currentAssistantEl) {
      var el = document.createElement('div');
      el.className = 'message message-assistant';
      el.innerHTML = '<div class="message-content"></div>';
      messagesEl.appendChild(el);
      state.currentAssistantEl = el;
      state.currentTextContent = '';
      state.toolCounter = 0;
    }
    return state.currentAssistantEl;
  }

  function renderMarkdown(text) {
    if (typeof marked !== 'undefined') {
      var html = marked.parse(text);
      html = html.replace(/<pre><code class="language-(\w+)">/g, function (m, lang) {
        return '<div class="code-header"><span>' + lang + '</span><button class="copy-btn" onclick="copyCode(this)">Copy</button></div><pre><code class="language-' + lang + '">';
      });
      return html;
    }
    return '<p>' + escapeHtml(text) + '</p>';
  }

  function escapeHtml(text) {
    var div = document.createElement('div');
    div.textContent = text;
    return div.innerHTML;
  }

  // --- Tool blocks ---
  function createToolBlock(name, input) {
    var id = 'tool-' + (++state.toolCounter);
    var summary = formatToolSummary(name, input);
    var inputStr = typeof input === 'object' ? JSON.stringify(input, null, 2) : String(input || '');
    // Check if this is a Write tool with a previewable file
    var previewBtn = '';
    if (name.toLowerCase() === 'write' && input && input.file_path) {
      var ext = input.file_path.split('.').pop().toLowerCase();
      if (['html', 'htm', 'svg', 'css'].indexOf(ext) !== -1) {
        previewBtn = '<button class="preview-btn tool-preview-btn" onclick="previewToolFile(\'' + escapeHtml(input.file_path).replace(/'/g, "\\'") + '\')">Preview</button>';
      }
    }
    var details = document.createElement('details');
    details.className = 'tool-block';
    details.id = id;
    details.innerHTML =
      '<summary>' +
        '<span class="tool-icon">' + getToolIcon(name) + '</span>' +
        '<span class="tool-name">' + escapeHtml(name) + '</span>' +
        '<span class="tool-summary">' + escapeHtml(summary) + '</span>' +
        previewBtn +
        '<span class="tool-status"><span class="tool-spinner"></span></span>' +
      '</summary>' +
      '<div class="tool-content">' + escapeHtml(inputStr) + '</div>';
    return details;
  }

  window.previewToolFile = function (path) {
    send('read_file', { path: path });
  };

  function formatToolSummary(name, input) {
    if (!input || typeof input !== 'object') return '';
    var n = name.toLowerCase();
    if (n === 'bash' || n === 'execute') return input.command || '';
    if (n === 'read') return input.file_path || '';
    if (n === 'write' || n === 'edit') return input.file_path || '';
    if (n === 'glob') return input.pattern || '';
    if (n === 'grep') return input.pattern || '';
    return input.file_path || input.path || input.command || '';
  }

  function getToolIcon(name) {
    var icons = { bash: '\u2318', execute: '\u2318', read: '\uD83D\uDCC4', write: '\u270F\uFE0F', edit: '\u270F\uFE0F', glob: '\uD83D\uDD0D', grep: '\uD83D\uDD0D', agent: '\uD83E\uDD16' };
    return icons[name.toLowerCase()] || '\uD83D\uDD27';
  }

  function updateLastToolResult(content, isError) {
    var blocks = messagesEl.querySelectorAll('.tool-block');
    if (blocks.length === 0) return;
    var last = blocks[blocks.length - 1];
    var status = last.querySelector('.tool-status');
    if (status) {
      status.innerHTML = isError ? '<span class="tool-error">\u2717</span>' : '<span class="tool-check">\u2713</span>';
    }
    if (content) {
      var contentEl = last.querySelector('.tool-content');
      if (contentEl) {
        contentEl.textContent = content.length > 2000 ? content.slice(0, 2000) + '\n... (truncated)' : content;
      }
    }
  }

  window.copyCode = function (btn) {
    var pre = btn.closest('.code-header').nextElementSibling;
    if (pre) {
      navigator.clipboard.writeText(pre.textContent).then(function () {
        btn.textContent = 'Copied!';
        setTimeout(function () { btn.textContent = 'Copy'; }, 1500);
      });
    }
  };

  // --- Time formatting ---
  function formatRelativeTime(iso) {
    if (!iso) return '';
    var diff = Date.now() - new Date(iso).getTime();
    var mins = Math.floor(diff / 60000);
    if (mins < 1) return 'now';
    if (mins < 60) return mins + 'm';
    var hours = Math.floor(mins / 60);
    if (hours < 24) return hours + 'h';
    var days = Math.floor(hours / 24);
    if (days < 7) return days + 'd';
    return new Date(iso).toLocaleDateString('en', { month: 'short', day: 'numeric' });
  }

  // --- Model name formatting ---
  function formatModelName(model) {
    if (!model) return 'Claude';
    if (model.includes('opus')) return 'Opus';
    if (model.includes('sonnet')) return 'Sonnet';
    if (model.includes('haiku')) return 'Haiku';
    return model.split('-')[0] || 'Claude';
  }

  // --- Main event handler (called from Rust) ---
  window.__onClaudeEvent = function (data) {
    var event;
    if (typeof data === 'string') {
      try { event = JSON.parse(data); } catch (e) { return; }
    } else {
      event = data;
    }

    var type = event.type;

    if (type === 'system') {
      if (event.subtype === 'init') {
        state.isReady = true;
        if (event.model) {
          state.modelName = event.model;
          modelDisplayEl.textContent = formatModelName(event.model);
        }
      }
      return;
    }

    if (type === 'clear') {
      // Handled by startNewChat — don't clear messages here since
      // load_conversation triggers new_chat which sends clear
      return;
    }

    if (type === 'error') {
      showError(event.message || 'An error occurred');
      setStreaming(false);
      return;
    }

    if (type === 'assistant') {
      var msg = event.message;
      if (!msg || !msg.content) return;
      var contents = Array.isArray(msg.content) ? msg.content : [msg.content];
      var assistantEl = getOrCreateAssistantMessage();
      var contentEl = assistantEl.querySelector('.message-content');

      for (var i = 0; i < contents.length; i++) {
        var block = contents[i];
        if (block.type === 'text' && block.text) {
          state.currentTextContent += block.text;
          var textEl = contentEl.querySelector('.assistant-text');
          if (!textEl) {
            textEl = document.createElement('div');
            textEl.className = 'assistant-text';
            contentEl.appendChild(textEl);
          }
          textEl.innerHTML = renderMarkdown(state.currentTextContent);
          thinkingEl.classList.add('hidden');
        } else if (block.type === 'tool_use') {
          contentEl.appendChild(createToolBlock(block.name, block.input));
          state.lastToolUse = { name: block.name, input: block.input };
          thinkingEl.classList.remove('hidden');
        }
      }
      autoScroll();
      return;
    }

    if (type === 'user') {
      var umsg = event.message;
      if (!umsg || !umsg.content) return;
      var ucontents = Array.isArray(umsg.content) ? umsg.content : [umsg.content];
      for (var j = 0; j < ucontents.length; j++) {
        if (ucontents[j].type === 'tool_result') {
          var rc = ucontents[j].content;
          var isErr = ucontents[j].is_error === true;
          updateLastToolResult(typeof rc === 'string' ? rc : JSON.stringify(rc), isErr);
          // Auto-preview: if Write tool created a renderable file, load it into Canvas
          if (!isErr && state.lastToolUse) {
            var tu = state.lastToolUse;
            var tn = (tu.name || '').toLowerCase();
            if (tn === 'write' && tu.input && tu.input.file_path) {
              var fp = tu.input.file_path;
              var ext = fp.split('.').pop().toLowerCase();
              if (['html', 'htm', 'svg', 'css'].indexOf(ext) !== -1) {
                send('read_file', { path: fp });
              }
            }
          }
        }
      }
      autoScroll();
      return;
    }

    if (type === 'result') {
      if (state.currentTextContent) {
        state.messages.push({
          role: 'assistant',
          content: state.currentTextContent,
          timestamp: new Date().toISOString(),
        });
        // Detect artifacts in the completed response
        var newArts = detectArtifacts(state.currentTextContent);
        newArts.forEach(function (art) {
          if (!state.artifacts.find(function (a) { return a.source === art.source; })) {
            state.artifacts.push(art);
          }
        });
        if (newArts.length > 0) renderArtifactList();
      }
      setStreaming(false);
      autoScroll();
      if (state.activeConversationId) saveCurrentConversation();
      return;
    }
  };

  function showError(message) {
    var el = document.createElement('div');
    el.className = 'error-banner';
    el.textContent = message;
    messagesEl.appendChild(el);
    autoScroll();
  }

  // ===== Canvas / Artifacts =====

  var canvasIframe = document.getElementById('canvas-iframe');
  var canvasSourceCode = document.getElementById('canvas-source-code');
  var canvasSourceEl = document.getElementById('canvas-source');
  var canvasPreviewEl = document.getElementById('canvas-preview');
  var artifactListEl = document.getElementById('artifact-list');
  var canvasCountEl = document.getElementById('canvas-count');
  var canvasArtifactTitle = document.getElementById('canvas-artifact-title');

  var RENDERABLE_LANGS = ['html', 'svg', 'htm'];

  function detectArtifacts(text) {
    var arts = [];
    var re = /```(html|svg|htm)\n([\s\S]*?)```/gi;
    var m;
    while ((m = re.exec(text)) !== null) {
      var lang = m[1].toLowerCase();
      var content = m[2].trim();
      arts.push({
        id: 'art-' + Date.now() + '-' + arts.length,
        type: lang === 'htm' ? 'html' : lang,
        title: extractTitle(content, lang) || (lang.toUpperCase() + ' Artifact'),
        content: content,
        source: content,
      });
    }
    return arts;
  }

  function extractTitle(content, lang) {
    if (lang === 'html' || lang === 'htm') {
      var m = content.match(/<title>(.*?)<\/title>/i);
      if (m) return m[1].substring(0, 40);
    }
    var cm = content.match(/<!--\s*(.*?)\s*-->/);
    return cm ? cm[1].substring(0, 40) : null;
  }

  function addArtifact(art) {
    state.artifacts.push(art);
    selectArtifact(art.id);
  }

  function selectArtifact(id) {
    state.activeArtifactId = id;
    var art = state.artifacts.find(function (a) { return a.id === id; });
    if (!art) return;
    canvasArtifactTitle.textContent = art.title;
    canvasIframe.srcdoc = art.type === 'svg'
      ? '<html><body style="margin:0;display:flex;align-items:center;justify-content:center;min-height:100vh;background:#fff">' + art.content + '</body></html>'
      : art.content;
    canvasSourceCode.textContent = art.source;
    if (typeof hljs !== 'undefined') hljs.highlightElement(canvasSourceCode);
    renderArtifactList();
  }

  function renderArtifactList() {
    artifactListEl.innerHTML = '';
    canvasCountEl.textContent = state.artifacts.length;
    state.artifacts.forEach(function (art) {
      var item = document.createElement('div');
      item.className = 'artifact-item' + (art.id === state.activeArtifactId ? ' active' : '');
      item.innerHTML = '<span class="artifact-type-badge">' + art.type + '</span><span>' + escapeHtml(art.title) + '</span>';
      item.addEventListener('click', function () { selectArtifact(art.id); });
      artifactListEl.appendChild(item);
    });
  }

  // Canvas toolbar
  document.getElementById('canvas-toggle-source').addEventListener('click', function () {
    canvasSourceEl.classList.remove('hidden');
    canvasPreviewEl.classList.add('hidden');
    this.classList.add('active');
    document.getElementById('canvas-toggle-preview').classList.remove('active');
  });
  document.getElementById('canvas-toggle-preview').addEventListener('click', function () {
    canvasPreviewEl.classList.remove('hidden');
    canvasSourceEl.classList.add('hidden');
    this.classList.add('active');
    document.getElementById('canvas-toggle-source').classList.remove('active');
  });
  document.getElementById('canvas-copy-btn').addEventListener('click', function () {
    var art = state.artifacts.find(function (a) { return a.id === state.activeArtifactId; });
    if (art) navigator.clipboard.writeText(art.source);
  });

  // Global: open code block in Canvas
  window.openInCanvas = function (btn) {
    var pre = btn.closest('.code-header').nextElementSibling;
    if (!pre) return;
    var code = pre.textContent;
    var lang = btn.closest('.code-header').querySelector('span').textContent.toLowerCase();
    addArtifact({
      id: 'art-' + Date.now(),
      type: lang,
      title: extractTitle(code, lang) || lang.toUpperCase() + ' Preview',
      content: code,
      source: code,
    });
    document.querySelector('.mode-tab[data-mode="canvas"]').click();
  };

  window.toggleSvgSource = function (id) {
    var el = document.getElementById(id);
    if (el) el.classList.toggle('show-source');
  };

  window.openSvgInCanvas = function (id) {
    var el = document.getElementById(id);
    if (!el) return;
    var svg = el.querySelector('.inline-svg-render').innerHTML;
    addArtifact({ id: 'art-' + Date.now(), type: 'svg', title: 'SVG Artifact', content: svg, source: svg });
    document.querySelector('.mode-tab[data-mode="canvas"]').click();
  };

  // Inline SVG rendering helper
  function sanitizeSVG(text) {
    return text.replace(/<script[\s\S]*?<\/script>/gi, '').replace(/\bon\w+\s*=\s*["'][^"']*["']/gi, '');
  }

  function decodeEntities(text) {
    var el = document.createElement('textarea');
    el.innerHTML = text;
    return el.value;
  }

  // Override renderMarkdown to add Preview buttons and inline SVG
  var _origRenderMarkdown = renderMarkdown;
  renderMarkdown = function (text) {
    var html = _origRenderMarkdown(text);
    // Add Preview button to renderable code blocks
    html = html.replace(/<div class="code-header"><span>(html|svg|htm)<\/span>([\s\S]*?)<\/div>/gi, function (m, lang, rest) {
      return '<div class="code-header"><span>' + lang + '</span><div><button class="preview-btn" onclick="openInCanvas(this)">Preview</button>' + rest.replace(/^<div>|<\/div>$/g, '') + '</div></div>';
    });
    // Inline SVG: replace svg code blocks with rendered preview
    html = html.replace(
      /<div class="code-header"><span>svg<\/span>[\s\S]*?<\/div>\s*<pre><code class="language-svg">([\s\S]*?)<\/code><\/pre>/gi,
      function (match, code) {
        var decoded = decodeEntities(code);
        var sanitized = sanitizeSVG(decoded);
        var sid = 'svg-' + (++state.toolCounter);
        return '<div class="inline-svg-container" id="' + sid + '">' +
          '<div class="inline-svg-header"><span>SVG</span><div class="inline-svg-actions">' +
            '<button class="inline-svg-btn" onclick="toggleSvgSource(\'' + sid + '\')">Source</button>' +
            '<button class="inline-svg-btn" onclick="openSvgInCanvas(\'' + sid + '\')">Canvas</button>' +
          '</div></div>' +
          '<div class="inline-svg-render">' + sanitized + '</div>' +
          '<div class="inline-svg-source"><pre><code>' + code + '</code></pre></div></div>';
      }
    );
    return html;
  };

  // ===== File Browser =====

  document.getElementById('files-toggle').addEventListener('click', function () {
    this.classList.toggle('expanded');
    var tree = document.getElementById('file-tree');
    tree.classList.toggle('hidden');
    if (!tree.classList.contains('hidden') && !state.fileTreeLoaded) {
      state.fileTreeLoaded = true;
      send('get_cwd');
    }
  });

  window.__onCwd = function (cwd) {
    state.fileCwd = cwd;
    document.getElementById('file-tree-path').textContent = cwd;
    send('list_files', { path: cwd });
  };

  window.__onFiles = function (data) {
    var container = document.getElementById('file-tree-contents');
    container.innerHTML = '';
    var sorted = data.entries.slice().sort(function (a, b) {
      if (a.is_dir !== b.is_dir) return a.is_dir ? -1 : 1;
      return a.name.localeCompare(b.name);
    });
    sorted.forEach(function (entry) {
      var el = document.createElement('div');
      el.className = 'file-entry';
      var icon = entry.is_dir ? '\uD83D\uDCC1' : getFileIcon(entry.name);
      el.innerHTML = '<span class="file-icon">' + icon + '</span>' +
        '<span class="file-name">' + escapeHtml(entry.name) + '</span>' +
        (entry.is_dir ? '' : '<span class="file-size">' + formatFileSize(entry.size) + '</span>');
      if (entry.is_dir) {
        el.addEventListener('click', function () { send('list_files', { path: data.path + '/' + entry.name }); });
      } else {
        el.addEventListener('click', function () { send('read_file', { path: data.path + '/' + entry.name }); });
      }
      container.appendChild(el);
    });
    document.getElementById('file-tree-path').textContent = data.path;
  };

  window.__onFileContent = function (data) {
    var ext = data.name.split('.').pop().toLowerCase();
    if (['html', 'htm', 'svg'].indexOf(ext) !== -1) {
      addArtifact({ id: 'file-' + Date.now(), type: ext === 'htm' ? 'html' : ext, title: data.name, content: data.content, source: data.content });
    } else {
      addArtifact({
        id: 'file-' + Date.now(),
        type: 'code',
        title: data.name,
        content: '<html><body style="margin:0;background:#1a1a1a;color:#ececec;font-family:monospace;font-size:13px;padding:16px;white-space:pre-wrap">' + escapeHtml(data.content) + '</body></html>',
        source: data.content,
      });
    }
    document.querySelector('.mode-tab[data-mode="canvas"]').click();
  };

  function getFileIcon(name) {
    var ext = name.split('.').pop().toLowerCase();
    var m = { rs: '\uD83E\uDD80', js: '\uD83D\uDFE1', ts: '\uD83D\uDD35', html: '\uD83C\uDF10', css: '\uD83C\uDFA8', py: '\uD83D\uDC0D', json: '{}', md: '\uD83D\uDCDD', svg: '\uD83D\uDDBC' };
    return m[ext] || '\uD83D\uDCC4';
  }

  function formatFileSize(bytes) {
    if (!bytes) return '';
    if (bytes < 1024) return bytes + 'B';
    if (bytes < 1048576) return (bytes / 1024).toFixed(1) + 'K';
    return (bytes / 1048576).toFixed(1) + 'M';
  }

  // ===== Browse Panel =====

  document.getElementById('browse-url').addEventListener('keydown', function (e) {
    if (e.key === 'Enter') {
      var url = this.value.trim();
      if (url && !url.match(/^https?:\/\//)) url = 'https://' + url;
      send('browser_navigate', { url: url });
      document.getElementById('browse-status').textContent = 'Loading...';
    }
  });

  document.getElementById('browse-back').addEventListener('click', function () { send('browser_back'); });
  document.getElementById('browse-forward').addEventListener('click', function () { send('browser_forward'); });
  document.getElementById('browse-refresh').addEventListener('click', function () { send('browser_refresh'); });
  document.getElementById('browse-screenshot').addEventListener('click', function () { send('browser_screenshot'); });

  window.__onBrowserReady = function () {
    document.getElementById('browse-status').textContent = 'Browser ready';
  };

  window.__onBrowserUrlChanged = function (data) {
    document.getElementById('browse-url').value = data.url || '';
    document.getElementById('browse-status').textContent = '';
  };

  window.__onBrowserScreenshot = function (data) {
    var img = document.createElement('img');
    img.src = 'data:image/jpeg;base64,' + data.image_base64;
    img.style.maxWidth = '100%';
    // Add as artifact
    addArtifact({
      id: 'screenshot-' + Date.now(),
      type: 'html',
      title: 'Screenshot',
      content: '<html><body style="margin:0;background:#000;display:flex;align-items:center;justify-content:center"><img src="data:image/jpeg;base64,' + data.image_base64 + '" style="max-width:100%"></body></html>',
      source: '(screenshot binary)',
    });
    document.querySelector('.mode-tab[data-mode="canvas"]').click();
  };

  // --- Keyboard shortcuts ---
  document.addEventListener('keydown', function (e) {
    if ((e.ctrlKey || e.metaKey) && e.key === 'n') {
      e.preventDefault();
      startNewChat();
    }
    if ((e.ctrlKey || e.metaKey) && e.key === 'k') {
      e.preventDefault();
      searchInput.focus();
    }
  });

  // --- Startup ---
  send('list_conversations');
  inputEl.focus();

})();

