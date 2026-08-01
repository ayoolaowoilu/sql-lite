interface QueryResult {
  columns: string[];
  rows: any[][];
  duration_ms: number;
  row_count: number;
  error: string | null;
  message: string | null;
}

interface QueryRecord {
  sql: string;
  timestamp: string;
  duration_ms: number;
  row_count: number;
  error: string | null;
}

interface LogEntry {
  time: string;
  level: string;
  message: string;
}

interface DbInfo {
  name: string;
}

interface DatabasesResponse {
  databases: DbInfo[];
  active: string | null;
}

interface ColInfo {
  name: string;
  dtype: string;
}

interface TableInfo {
  name: string;
  columns: ColInfo[];
}

interface SchemaResponse {
  tables: TableInfo[];
  active_db: string | null;
}

class SQLApp {
  private queryInput: HTMLTextAreaElement;
  private runBtn: HTMLButtonElement;
  private clearBtn: HTMLButtonElement;
  private statusEl: HTMLElement;
  private resultsTable: HTMLTableElement;
  private historyList: HTMLElement;
  private logsContent: HTMLElement;
  private dbList: HTMLElement;
  private dbTree: HTMLElement;
  private dbBadge: HTMLElement;
  private newDbInput: HTMLInputElement;
  private createDbBtn: HTMLButtonElement;
  private tabs: NodeListOf<Element>;
  private activeDb: string | null = null;

  constructor() {
    this.queryInput = document.getElementById('queryInput') as HTMLTextAreaElement;
    this.runBtn = document.getElementById('runBtn') as HTMLButtonElement;
    this.clearBtn = document.getElementById('clearBtn') as HTMLButtonElement;
    this.statusEl = document.getElementById('status') as HTMLElement;
    this.resultsTable = document.getElementById('resultsTable') as HTMLTableElement;
    this.historyList = document.getElementById('historyList') as HTMLElement;
    this.logsContent = document.getElementById('logsContent') as HTMLElement;
    this.dbList = document.getElementById('dbList') as HTMLElement;
    this.dbTree = document.getElementById('dbTree') as HTMLElement;
    this.dbBadge = document.getElementById('dbBadge') as HTMLElement;
    this.newDbInput = document.getElementById('newDbInput') as HTMLInputElement;
    this.createDbBtn = document.getElementById('createDbBtn') as HTMLButtonElement;
    this.tabs = document.querySelectorAll('.tab');
    this.init();
  }

  private init(): void {
    this.runBtn.addEventListener('click', () => this.runQuery());
    this.clearBtn.addEventListener('click', () => this.clearQuery());
    this.createDbBtn.addEventListener('click', () => this.createDatabase());
    this.newDbInput.addEventListener('keydown', (e: KeyboardEvent) => {
      if (e.key === 'Enter') this.createDatabase();
    });
    this.queryInput.addEventListener('keydown', (e: KeyboardEvent) => {
      if (e.ctrlKey && e.key === 'Enter') this.runQuery();
    });

    this.tabs.forEach(tab => {
      tab.addEventListener('click', () => this.switchTab(tab as HTMLElement));
    });

    this.loadDatabases();
    this.loadHistory();
    this.loadLogs();

    setInterval(() => this.loadLogs(), 2000);
  }

  private async loadDatabases(): Promise<void> {
    try {
      const res = await fetch('/api/databases');
      const data: DatabasesResponse = await res.json();
      this.activeDb = data.active;
      this.renderDbList(data.databases);
      this.updateDbBadge();
      if (this.activeDb) {
        this.loadSchema();
      } else {
        this.dbTree.innerHTML = '<div class="empty-state">Select a database</div>';
      }
    } catch (err: any) {
      this.log('ERROR', 'Failed to load databases: ' + err.message);
    }
  }

  private renderDbList(databases: DbInfo[]): void {
    this.dbList.innerHTML = '';
    if (databases.length === 0) {
      this.dbList.innerHTML = '<div class="empty-state" style="padding: 20px; font-size: 12px;">No databases yet</div>';
      return;
    }
    databases.forEach(db => {
      const item = document.createElement('div');
      item.className = 'db-item' + (db.name === this.activeDb ? ' active' : '');
      item.innerHTML = `
        <span class="db-name">${this.escapeHtml(db.name)}</span>
        <button class="db-drop" title="Drop database">✕</button>
      `;
      item.querySelector('.db-name')!.addEventListener('click', () => {
        this.useDatabase(db.name);
      });
      item.querySelector('.db-drop')!.addEventListener('click', (e: Event) => {
        e.stopPropagation();
        this.dropDatabase(db.name);
      });
      this.dbList.appendChild(item);
    });
  }

  private async createDatabase(): Promise<void> {
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
    } catch (err: any) {
      this.log('ERROR', err.message);
    }
  }

  private async useDatabase(name: string): Promise<void> {
    await this.runRawSql(`USE ${name};`);
  }

  private async dropDatabase(name: string): Promise<void> {
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
    } catch (err: any) {
      this.log('ERROR', err.message);
    }
  }

  private updateDbBadge(): void {
    if (this.activeDb) {
      this.dbBadge.textContent = this.activeDb;
      this.dbBadge.classList.add('active');
    } else {
      this.dbBadge.textContent = 'No database selected';
      this.dbBadge.classList.remove('active');
    }
  }

  private async loadSchema(): Promise<void> {
    if (!this.activeDb) return;
    try {
      const res = await fetch('/api/schema');
      const data: SchemaResponse = await res.json();
      this.dbTree.innerHTML = '';
      if (data.tables.length === 0) {
        this.dbTree.innerHTML = '<div class="empty-state" style="padding: 20px; font-size: 12px;">No tables</div>';
        return;
      }
      data.tables.forEach(table => {
        const item = document.createElement('div');
        item.className = 'table-item';
        item.textContent = table.name;
        item.title = table.columns.map(c => `${c.name} (${c.dtype})`).join(', ');
        item.addEventListener('click', () => {
          this.queryInput.value = `SELECT * FROM ${table.name} LIMIT 100;`;
          this.runQuery();
        });
        this.dbTree.appendChild(item);
      });
    } catch (err: any) {
      this.log('ERROR', 'Failed to load schema: ' + err.message);
    }
  }

  private async runQuery(): Promise<void> {
    const sql = this.queryInput.value.trim();
    if (!sql) return;
    await this.runRawSql(sql);
  }

  private async runRawSql(sql: string): Promise<void> {
    this.statusEl.textContent = 'Running...';
    this.runBtn.disabled = true;

    try {
      const res = await fetch('/api/query', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ sql })
      });

      const data: QueryResult = await res.json();

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
    } catch (err: any) {
      this.statusEl.textContent = 'Network error';
      this.statusEl.style.color = 'var(--error)';
      this.log('ERROR', 'Query failed: ' + err.message);
    } finally {
      this.runBtn.disabled = false;
    }
  }

  private renderResults(data: QueryResult): void {
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
    this.switchTab(document.querySelector('[data-tab="results"]') as HTMLElement);
  }

  private renderMessage(msg: string): void {
    this.resultsTable.innerHTML = `<div class="message-box">${this.escapeHtml(msg)}</div>`;
    this.switchTab(document.querySelector('[data-tab="results"]') as HTMLElement);
  }

  private renderError(msg: string): void {
    this.resultsTable.innerHTML = `<div class="message-box error">⚠️ ${this.escapeHtml(msg)}</div>`;
    this.switchTab(document.querySelector('[data-tab="results"]') as HTMLElement);
  }

  private clearQuery(): void {
    this.queryInput.value = '';
    this.queryInput.focus();
  }

  private async loadHistory(): Promise<void> {
    try {
      const res = await fetch('/api/history');
      const data = await res.json();
      this.historyList.innerHTML = '';
      [...data.queries].reverse().forEach((q: QueryRecord) => {
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

  private async loadLogs(): Promise<void> {
    try {
      const res = await fetch('/api/logs');
      const data = await res.json();
      this.logsContent.innerHTML = '';
      data.logs.forEach((log: LogEntry) => {
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

  private log(level: string, message: string): void {
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

  private switchTab(tab: HTMLElement): void {
    const target = tab.dataset.tab!;
    this.tabs.forEach(t => t.classList.remove('active'));
    tab.classList.add('active');
    document.querySelectorAll('.tab-content').forEach(c => c.classList.remove('active'));
    document.getElementById(`${target}Tab`)!.classList.add('active');
  }

  private escapeHtml(text: string): string {
    const div = document.createElement('div');
    div.textContent = text;
    return div.innerHTML;
  }
}

new SQLApp();
