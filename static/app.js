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
    this.activeDb = null;
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

    this.loadDatabases();
    this.loadHistory();
    this.loadLogs();

    setInterval(() => this.loadLogs(), 2000);
  }

  async loadDatabases() {
    try {
      const res = await fetch('/api/databases');
      const data = await res.json();
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