class SQLApp {
  constructor() {
    this.queryInput = document.getElementById('queryInput');
    this.runBtn = document.getElementById('runBtn');
    this.clearBtn = document.getElementById('clearBtn');
    this.statusEl = document.getElementById('status');
    this.resultsTable = document.getElementById('resultsTable');
    this.historyList = document.getElementById('historyList');
    this.logsContent = document.getElementById('logsContent');
    this.dbList = document.getElementById('dbList');
    this.dbTree = document.getElementById('dbTree');
    this.dbBadge = document.getElementById('dbBadge');
    this.newDbInput = document.getElementById('newDbInput');
    this.createDbBtn = document.getElementById('createDbBtn');
    this.tabs = document.querySelectorAll('.tab');
    this.passwordSetup = document.getElementById('passwordSetup');
    this.passwordSetupForm = document.getElementById('passwordSetupForm');
    this.passwordSetupError = document.getElementById('passwordSetupError');
    this.passwordSetupButton = document.getElementById('passwordSetupButton');
    this.mysqlPassword = document.getElementById('mysqlPassword');
    this.mysqlPasswordConfirmation = document.getElementById('mysqlPasswordConfirmation');
    this.connectionPicker = document.getElementById('connectionPicker');
    this.connectionList = document.getElementById('connectionList');
    this.addConnectionForm = document.getElementById('addConnectionForm');
    this.connectionFormError = document.getElementById('connectionFormError');
    this.connectionPasswordDialog = document.getElementById('connectionPasswordDialog');
    this.connectionPasswordForm = document.getElementById('connectionPasswordForm');
    this.connectionPasswordTitle = document.getElementById('connectionPasswordTitle');
    this.connectionPasswordDescription = document.getElementById('connectionPasswordDescription');
    this.connectionPassword = document.getElementById('connectionPassword');
    this.connectionPasswordError = document.getElementById('connectionPasswordError');
    this.connectButton = document.getElementById('connectButton');
    this.disconnectBtn = document.getElementById('disconnectBtn');
    this.connectionName = document.getElementById('connectionName');
    this.activeDb = null;
    this.pendingProfile = null;
    this.connectedProfile = null;
    this.workspaceStarted = false;
    this.init();
  }

  svgDatabase() {
    return `<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><ellipse cx="12" cy="5" rx="9" ry="3"/><path d="M3 5v14a9 3 0 0 0 18 0V5"/><path d="M3 12a9 3 0 0 0 18 0"/></svg>`;
  }

  svgTable() {
    return `<svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect x="3" y="3" width="18" height="18" rx="2"/><path d="M3 9h18"/><path d="M3 15h18"/><path d="M9 3v18"/><path d="M15 3v18"/></svg>`;
  }

  svgX() {
    return `<svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round"><line x1="18" y1="6" x2="6" y2="18"/><line x1="6" y1="6" x2="18" y2="18"/></svg>`;
  }

  init() {
    this.runBtn.addEventListener('click', () => this.runQuery());
    this.clearBtn.addEventListener('click', () => this.clearQuery());
    this.createDbBtn.addEventListener('click', () => this.createDatabase());
    this.newDbInput.addEventListener('keydown', (e) => {
      if (e.key === 'Enter') this.createDatabase();
    });
    this.queryInput.addEventListener('keydown', (e) => {
      if (e.ctrlKey && e.key === 'Enter') this.runQuery();
    });

    this.tabs.forEach(tab => {
      tab.addEventListener('click', () => this.switchTab(tab));
    });

    this.initResizers();
    this.passwordSetupForm.addEventListener('submit', (event) => {
      event.preventDefault();
      this.saveMysqlPassword();
    });
    this.addConnectionForm.addEventListener('submit', (event) => {
      event.preventDefault();
      this.addConnectionProfile();
    });
    this.connectionPasswordForm.addEventListener('submit', (event) => {
      event.preventDefault();
      this.connectSelectedProfile();
    });
    document.getElementById('cancelConnectionBtn').addEventListener('click', () => {
      this.connectionPasswordDialog.hidden = true;
      this.connectionPasswordForm.reset();
    });
    this.disconnectBtn.addEventListener('click', () => this.disconnectProfile());
    window.addEventListener('pagehide', () => {
      fetch('/api/connections/disconnect', { method: 'POST', keepalive: true }).catch(() => {});
    });
    this.checkMysqlSetup();
  }

  async checkMysqlSetup() {
    try {
      const disconnect = await fetch('/api/connections/disconnect', { method: 'POST' });
      if (!disconnect.ok) throw new Error('Could not clear the previous connection.');
      const response = await fetch('/api/mysql/status');
      const data = await response.json();
      if (!response.ok) throw new Error(data.error || 'Could not check MySQL setup.');
      if (data.configured) {
        this.showConnectionPicker();
      } else {
        this.passwordSetup.hidden = false;
        this.mysqlPassword.focus();
      }
    } catch (error) {
      this.showPasswordSetupError(error.message);
    }
  }

  async saveMysqlPassword() {
    this.passwordSetupError.hidden = true;
    this.passwordSetupButton.disabled = true;
    try {
      const response = await fetch('/api/mysql/password', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          password: this.mysqlPassword.value,
          confirmation: this.mysqlPasswordConfirmation.value
        })
      });
      const data = await response.json();
      if (!response.ok) throw new Error(data.error || 'Could not save the MySQL password.');
      const password = this.mysqlPassword.value;
      this.passwordSetupForm.reset();
      this.passwordSetup.hidden = true;
      try {
        await this.connectProfile(this.localProfile(), password);
      } catch (error) {
        this.showConnectionPicker();
        this.showConnectionPassword(this.localProfile());
        this.connectionPasswordError.textContent = error.message;
        this.connectionPasswordError.hidden = false;
      }
    } catch (error) {
      this.showPasswordSetupError(error.message);
    } finally {
      this.passwordSetupButton.disabled = false;
    }
  }

  showPasswordSetupError(message) {
    this.passwordSetup.hidden = false;
    this.passwordSetupError.textContent = message;
    this.passwordSetupError.hidden = false;
  }

  localProfile() {
    return {
      id: 'local-mysqlite',
      name: 'MySQLite Local',
      kind: 'local',
      host: '127.0.0.1',
      port: 9995,
      user: 'root'
    };
  }

  loadConnectionProfiles() {
    const local = this.localProfile();
    try {
      const stored = JSON.parse(localStorage.getItem('mysqlite-connections') || '[]');
      const profiles = Array.isArray(stored)
        ? stored.filter(profile => profile && profile.kind === 'mysql' && profile.id && profile.name && profile.host && profile.user)
        : [];
      return [local, ...profiles.filter(profile => profile.id !== local.id)];
    } catch (error) {
      this.connectionFormError.textContent = `Could not load saved connections: ${error.message}`;
      this.connectionFormError.hidden = false;
      return [local];
    }
  }

  saveConnectionProfiles(profiles) {
    localStorage.setItem('mysqlite-connections', JSON.stringify(
      profiles.filter(profile => profile.kind === 'mysql')
    ));
  }

  showConnectionPicker() {
    this.passwordSetup.hidden = true;
    this.connectionPicker.hidden = false;
    this.renderConnectionProfiles();
  }

  renderConnectionProfiles() {
    const profiles = this.loadConnectionProfiles();
    this.connectionList.replaceChildren();
    profiles.forEach(profile => {
      const card = document.createElement('article');
      card.className = 'connection-card';

      const icon = document.createElement('span');
      icon.className = 'connection-card-icon';
      icon.innerHTML = this.svgDatabase();

      const details = document.createElement('span');
      details.className = 'connection-card-details';
      const name = document.createElement('strong');
      name.textContent = profile.name;
      const address = document.createElement('span');
      address.textContent = `${profile.user}@${profile.host}:${profile.port}`;
      details.append(name, address);

      const actions = document.createElement('div');
      actions.className = 'connection-card-actions';
      const connect = document.createElement('button');
      connect.type = 'button';
      connect.className = 'connect-profile';
      connect.textContent = 'Connect';
      connect.addEventListener('click', () => this.showConnectionPassword(profile));
      actions.appendChild(connect);

      if (profile.kind !== 'local') {
        const remove = document.createElement('button');
        remove.type = 'button';
        remove.className = 'delete-profile';
        remove.textContent = 'Remove';
        remove.setAttribute('aria-label', `Remove ${profile.name}`);
        remove.addEventListener('click', () => this.removeConnectionProfile(profile.id));
        actions.appendChild(remove);
      }

      card.append(icon, details, actions);
      this.connectionList.appendChild(card);
    });
  }

  addConnectionProfile() {
    this.connectionFormError.hidden = true;
    const formData = new FormData(this.addConnectionForm);
    const profile = {
      id: `mysql-${Date.now()}-${Math.random().toString(16).slice(2)}`,
      name: String(formData.get('name')).trim(),
      kind: 'mysql',
      host: String(formData.get('host')).trim(),
      port: Number(formData.get('port')),
      user: String(formData.get('user')).trim()
    };
    if (!profile.name || !profile.host || !profile.user || !Number.isInteger(profile.port) || profile.port < 1 || profile.port > 65535) {
      this.connectionFormError.textContent = 'Enter a name, hostname, username, and valid port.';
      this.connectionFormError.hidden = false;
      return;
    }
    try {
      const profiles = this.loadConnectionProfiles();
      this.saveConnectionProfiles([...profiles, profile]);
      this.addConnectionForm.reset();
      document.getElementById('profileHost').value = '127.0.0.1';
      document.getElementById('profilePort').value = '3306';
      document.getElementById('profileUser').value = 'root';
      this.renderConnectionProfiles();
    } catch (error) {
      this.connectionFormError.textContent = `Could not save this connection: ${error.message}`;
      this.connectionFormError.hidden = false;
    }
  }

  removeConnectionProfile(id) {
    try {
      this.saveConnectionProfiles(this.loadConnectionProfiles().filter(profile => profile.id !== id));
      this.renderConnectionProfiles();
    } catch (error) {
      this.connectionFormError.textContent = `Could not remove this connection: ${error.message}`;
      this.connectionFormError.hidden = false;
    }
  }

  showConnectionPassword(profile) {
    this.pendingProfile = profile;
    this.connectionPasswordTitle.textContent = `Connect to ${profile.name}`;
    this.connectionPasswordDescription.textContent = `${profile.user}@${profile.host}:${profile.port}`;
    this.connectionPasswordError.hidden = true;
    this.connectionPasswordForm.reset();
    this.connectionPasswordDialog.hidden = false;
    this.connectionPassword.focus();
  }

  async connectSelectedProfile() {
    if (!this.pendingProfile) return;
    this.connectButton.disabled = true;
    this.connectionPasswordError.hidden = true;
    try {
      await this.connectProfile(this.pendingProfile, this.connectionPassword.value);
      this.connectionPasswordForm.reset();
      this.connectionPasswordDialog.hidden = true;
    } catch (error) {
      this.connectionPasswordError.textContent = error.message;
      this.connectionPasswordError.hidden = false;
    } finally {
      this.connectButton.disabled = false;
    }
  }

  async connectProfile(profile, password) {
    const response = await fetch('/api/connections/connect', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ profile, password })
    });
    const data = await response.json();
    if (!response.ok) throw new Error(data.error || 'Could not connect to this server.');
    this.pendingProfile = null;
    this.startWorkspace(profile);
  }

  async disconnectProfile() {
    try {
      const response = await fetch('/api/connections/disconnect', { method: 'POST' });
      if (!response.ok) throw new Error('Could not disconnect from the server.');
      this.connectedProfile = null;
      this.activeDb = null;
      this.connectionName.hidden = true;
      this.disconnectBtn.hidden = true;
      this.connectionPicker.hidden = false;
      this.renderConnectionProfiles();
    } catch (error) {
      this.log('ERROR', error.message);
    }
  }

  startWorkspace(profile) {
    this.passwordSetup.hidden = true;
    this.connectionPicker.hidden = true;
    this.connectionPasswordDialog.hidden = true;
    this.connectedProfile = profile;
    this.connectionName.textContent = profile.name;
    this.connectionName.hidden = false;
    this.disconnectBtn.hidden = false;
    this.loadDatabases();
    this.loadHistory();
    this.loadLogs();
    if (this.workspaceStarted) return;
    this.workspaceStarted = true;

    setInterval(() => this.loadLogs(), 2000);
  }

  initResizers() {
    const sidebar = document.getElementById('sidebar');
    const sidebarResize = document.getElementById('sidebarResize');
    const querySection = document.getElementById('querySection');
    const queryResize = document.getElementById('queryResize');
    const logsSection = document.getElementById('logsSection');
    const logsResize = document.getElementById('logsResize');

    let resizing = null;
    let startPos = 0;
    let startSize = 0;

    const onMouseMove = (e) => {
      if (!resizing) return;
      e.preventDefault();

      if (resizing === 'sidebar') {
        const dx = e.clientX - startPos;
        const newWidth = Math.max(180, Math.min(500, startSize + dx));
        sidebar.style.width = newWidth + 'px';
      } else if (resizing === 'query') {
        const dy = e.clientY - startPos;
        const newHeight = Math.max(80, Math.min(400, startSize + dy));
        querySection.style.height = newHeight + 'px';
      } else if (resizing === 'logs') {
        const dy = startPos - e.clientY;
        const newHeight = Math.max(80, Math.min(400, startSize + dy));
        logsSection.style.height = newHeight + 'px';
      }
    };

    const onMouseUp = () => {
      if (!resizing) return;
      if (sidebarResize) sidebarResize.classList.remove('resizing');
      if (queryResize) queryResize.classList.remove('resizing');
      if (logsResize) logsResize.classList.remove('resizing');
      document.body.style.cursor = '';
      document.body.style.userSelect = '';
      resizing = null;
    };

    if (sidebarResize) {
      sidebarResize.addEventListener('mousedown', (e) => {
        resizing = 'sidebar';
        startPos = e.clientX;
        startSize = sidebar.offsetWidth;
        sidebarResize.classList.add('resizing');
        document.body.style.cursor = 'col-resize';
        document.body.style.userSelect = 'none';
      });
    }

    if (queryResize) {
      queryResize.addEventListener('mousedown', (e) => {
        resizing = 'query';
        startPos = e.clientY;
        startSize = querySection.offsetHeight;
        queryResize.classList.add('resizing');
        document.body.style.cursor = 'row-resize';
        document.body.style.userSelect = 'none';
      });
    }

    if (logsResize) {
      logsResize.addEventListener('mousedown', (e) => {
        resizing = 'logs';
        startPos = e.clientY;
        startSize = logsSection.offsetHeight;
        logsResize.classList.add('resizing');
        document.body.style.cursor = 'row-resize';
        document.body.style.userSelect = 'none';
      });
    }

    document.addEventListener('mousemove', onMouseMove);
    document.addEventListener('mouseup', onMouseUp);
  }

  async loadDatabases() {
    try {
      const res = await fetch('/api/databases');
      const data = await res.json();
      if (!res.ok) throw new Error(data.error || 'Failed to load databases.');
      this.activeDb = data.active;
      this.renderDbList(data.databases);
      this.updateDbBadge();
      if (this.activeDb) {
        this.loadSchema();
      } else {
        this.dbTree.innerHTML = '<div class="empty-state">Select a database</div>';
      }
    } catch (err) {
      this.log('ERROR', 'Failed to load databases: ' + err.message);
    }
  }

  renderDbList(databases) {
    this.dbList.innerHTML = '';
    if (databases.length === 0) {
      this.dbList.innerHTML = '<div class="empty-state" style="padding: 20px; font-size: 12px;">No databases yet</div>';
      return;
    }
    databases.forEach(db => {
      const item = document.createElement('div');
      item.className = 'db-item' + (db.name === this.activeDb ? ' active' : '');
      item.innerHTML = `
        <span class="db-name">${this.svgDatabase()}${this.escapeHtml(db.name)}</span>
        <button class="db-drop" title="Drop database">${this.svgX()}</button>
      `;
      item.querySelector('.db-name').addEventListener('click', () => {
        this.useDatabase(db.name);
      });
      item.querySelector('.db-drop').addEventListener('click', (e) => {
        e.stopPropagation();
        this.dropDatabase(db.name);
      });
      this.dbList.appendChild(item);
    });
  }

  async createDatabase() {
    const name = this.newDbInput.value.trim();
    if (!name) return;
    try {
      const res = await fetch('/api/databases', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ sql: name })
      });
      const data = await res.json();
      if (data.ok) {
        this.newDbInput.value = '';
        this.log('INFO', data.message);
        this.loadDatabases();
      } else {
        this.log('ERROR', data.error);
      }
    } catch (err) {
      this.log('ERROR', err.message);
    }
  }

  async useDatabase(name) {
    await this.runRawSql(`USE ${name};`);
  }

  async dropDatabase(name) {
    if (!confirm(`Drop database "${name}"? All data will be lost.`)) return;
    try {
      const res = await fetch(`/api/databases/${encodeURIComponent(name)}`, { method: 'DELETE' });
      const data = await res.json();
      if (data.ok) {
        this.log('INFO', data.message);
        this.loadDatabases();
      } else {
        this.log('ERROR', data.error);
      }
    } catch (err) {
      this.log('ERROR', err.message);
    }
  }

  updateDbBadge() {
    if (this.activeDb) {
      this.dbBadge.textContent = this.activeDb;
      this.dbBadge.classList.add('active');
    } else {
      this.dbBadge.textContent = 'No database selected';
      this.dbBadge.classList.remove('active');
    }
  }

  async loadSchema() {
    if (!this.activeDb) return;
    try {
      const res = await fetch('/api/schema');
      const data = await res.json();
      this.dbTree.innerHTML = '';
      if (data.tables.length === 0) {
        this.dbTree.innerHTML = '<div class="empty-state" style="padding: 20px; font-size: 12px;">No tables</div>';
        return;
      }
      data.tables.forEach(table => {
        const item = document.createElement('div');
        item.className = 'table-item';
        item.innerHTML = `${this.svgTable()}${this.escapeHtml(table.name)}`;
        item.title = table.columns.map(c => `${c.name} (${c.dtype})`).join(', ');
        item.addEventListener('click', () => {
          this.queryInput.value = `SELECT * FROM ${table.name} LIMIT 100;`;
          this.runQuery();
        });
        this.dbTree.appendChild(item);
      });
    } catch (err) {
      this.log('ERROR', 'Failed to load schema: ' + err.message);
    }
  }

  async runQuery() {
    const sql = this.queryInput.value.trim();
    if (!sql) return;
    await this.runRawSql(sql);
  }

  async runRawSql(sql) {
    this.statusEl.textContent = 'Running...';
    this.runBtn.disabled = true;

    try {
      const res = await fetch('/api/query', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ sql })
      });

      const data = await res.json();

      if (data.error) {
        this.statusEl.textContent = `Error (${data.duration_ms}ms)`;
        this.statusEl.style.color = 'var(--error)';
        this.renderError(data.error);
      } else if (data.message) {
        this.statusEl.textContent = `OK (${data.duration_ms}ms)`;
        this.statusEl.style.color = 'var(--success)';
        this.renderMessage(data.message);
      } else {
        this.statusEl.textContent = `${data.row_count} rows (${data.duration_ms}ms)`;
        this.statusEl.style.color = 'var(--success)';
        this.renderResults(data);
      }

      const upper = sql.toUpperCase();
      if (upper.startsWith('CREATE DATABASE') || upper.startsWith('USE ') || upper.startsWith('DROP DATABASE')) {
        this.loadDatabases();
      } else if (upper.startsWith('CREATE TABLE') || upper.startsWith('DROP TABLE')) {
        this.loadSchema();
      }

      this.loadHistory();
      this.loadLogs();
    } catch (err) {
      this.statusEl.textContent = 'Network error';
      this.statusEl.style.color = 'var(--error)';
      this.log('ERROR', 'Query failed: ' + err.message);
    } finally {
      this.runBtn.disabled = false;
    }
  }

  renderResults(data) {
    if (data.columns.length === 0) {
      this.resultsTable.innerHTML = '<div class="empty-state">No results</div>';
      return;
    }
    let html = '<thead><tr>';
    data.columns.forEach(col => {
      html += `<th>${this.escapeHtml(col)}</th>`;
    });
    html += '</tr></thead><tbody>';
    data.rows.forEach(row => {
      html += '<tr>';
      row.forEach(cell => {
        if (cell === null) {
          html += '<td class="null-value">NULL</td>';
        } else {
          html += `<td>${this.escapeHtml(String(cell))}</td>`;
        }
      });
      html += '</tr>';
    });
    html += '</tbody>';
    this.resultsTable.innerHTML = html;
    this.switchTab(document.querySelector('[data-tab="results"]'));
  }

  renderMessage(msg) {
    this.resultsTable.innerHTML = `<div class="message-box">${this.escapeHtml(msg)}</div>`;
    this.switchTab(document.querySelector('[data-tab="results"]'));
  }

  renderError(msg) {
    this.resultsTable.innerHTML = `<div class="message-box error">⚠️ ${this.escapeHtml(msg)}</div>`;
    this.switchTab(document.querySelector('[data-tab="results"]'));
  }

  clearQuery() {
    this.queryInput.value = '';
    this.queryInput.focus();
  }

  async loadHistory() {
    try {
      const res = await fetch('/api/history');
      const data = await res.json();
      this.historyList.innerHTML = '';
      [...data.queries].reverse().forEach(q => {
        const item = document.createElement('div');
        item.className = `history-item ${q.error ? 'error' : 'success'}`;
        item.innerHTML = `
          <div class="history-sql">${this.escapeHtml(q.sql)}</div>
          <div class="history-meta">
            <span>${q.timestamp}</span>
            <span>${q.row_count} rows</span>
            <span>${q.duration_ms}ms</span>
            ${q.error ? '<span style="color: var(--error)">ERROR</span>' : ''}
          </div>
        `;
        item.addEventListener('click', () => {
          this.queryInput.value = q.sql;
        });
        this.historyList.appendChild(item);
      });
    } catch (err) {
      console.error('Failed to load history', err);
    }
  }

  async loadLogs() {
    try {
      const res = await fetch('/api/logs');
      const data = await res.json();
      this.logsContent.innerHTML = '';
      data.logs.forEach(log => {
        const entry = document.createElement('div');
        entry.className = 'log-entry';
        entry.innerHTML = `
          <span class="log-time">${log.time}</span>
          <span class="log-level ${log.level.toLowerCase()}">${log.level}</span>
          <span class="log-message">${this.escapeHtml(log.message)}</span>
        `;
        this.logsContent.appendChild(entry);
      });
      this.logsContent.scrollTop = this.logsContent.scrollHeight;
    } catch (err) {
      console.error('Failed to load logs', err);
    }
  }

  log(level, message) {
    const entry = document.createElement('div');
    entry.className = 'log-entry';
    const time = new Date().toLocaleTimeString();
    entry.innerHTML = `
      <span class="log-time">${time}</span>
      <span class="log-level ${level.toLowerCase()}">${level}</span>
      <span class="log-message">${this.escapeHtml(message)}</span>
    `;
    this.logsContent.appendChild(entry);
    this.logsContent.scrollTop = this.logsContent.scrollHeight;
  }

  switchTab(tab) {
    const target = tab.dataset.tab;
    this.tabs.forEach(t => t.classList.remove('active'));
    tab.classList.add('active');
    document.querySelectorAll('.tab-content').forEach(c => c.classList.remove('active'));
    document.getElementById(`${target}Tab`).classList.add('active');
  }

  escapeHtml(text) {
    const div = document.createElement('div');
    div.textContent = text;
    return div.innerHTML;
  }
}

new SQLApp();
