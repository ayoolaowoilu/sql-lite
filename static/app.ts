interface QueryResult {
  columns: string[];
  rows: any[][];
  duration_ms: number;
  row_count: number;
  error: string | null;
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

interface SchemaResponse {
  databases: {
    name: string;
    tables: { name: string; columns: string[] }[];
  }[];
}

class SQLApp {
  private queryInput: HTMLTextAreaElement;
  private runBtn: HTMLButtonElement;
  private clearBtn: HTMLButtonElement;
  private statusEl: HTMLElement;
  private resultsTable: HTMLTableElement;
  private historyList: HTMLElement;
  private logsContent: HTMLElement;
  private dbTree: HTMLElement;
  private tabs: NodeListOf<Element>;

  constructor() {
    this.queryInput = document.getElementById('queryInput') as HTMLTextAreaElement;
    this.runBtn = document.getElementById('runBtn') as HTMLButtonElement;
    this.clearBtn = document.getElementById('clearBtn') as HTMLButtonElement;
    this.statusEl = document.getElementById('status') as HTMLElement;
    this.resultsTable = document.getElementById('resultsTable') as HTMLTableElement;
    this.historyList = document.getElementById('historyList') as HTMLElement;
    this.logsContent = document.getElementById('logsContent') as HTMLElement;
    this.dbTree = document.getElementById('dbTree') as HTMLElement;
    this.tabs = document.querySelectorAll('.tab');
    this.init();
  }

  private init(): void {
    this.runBtn.addEventListener('click', () => this.runQuery());
    this.clearBtn.addEventListener('click', () => this.clearQuery());
    this.queryInput.addEventListener('keydown', (e: KeyboardEvent) => {
      if (e.ctrlKey && e.key === 'Enter') this.runQuery();
    });

    this.tabs.forEach(tab => {
      tab.addEventListener('click', () => this.switchTab(tab as HTMLElement));
    });

    this.loadSchema();
    this.loadHistory();
    this.loadLogs();

    setInterval(() => this.loadLogs(), 2000);
  }

  private async loadSchema(): Promise<void> {
    try {
      const res = await fetch('/api/schema');
      const data: SchemaResponse = await res.json();

      this.dbTree.innerHTML = '';
      data.databases.forEach(db => {
        const dbItem = document.createElement('div');
        dbItem.className = 'db-item expanded';

        const dbName = document.createElement('div');
        dbName.className = 'db-name';
        dbName.textContent = db.name;
        dbName.addEventListener('click', () => dbItem.classList.toggle('expanded'));

        const tableList = document.createElement('div');
        tableList.className = 'table-list';

        db.tables.forEach(table => {
          const tableItem = document.createElement('div');
          tableItem.className = 'table-item';
          tableItem.textContent = table.name;
          tableItem.title = table.columns.join(', ');
          tableItem.addEventListener('click', () => {
            this.queryInput.value = `SELECT * FROM ${table.name} LIMIT 100;`;
            this.runQuery();
          });
          tableList.appendChild(tableItem);
        });

        dbItem.appendChild(dbName);
        dbItem.appendChild(tableList);
        this.dbTree.appendChild(dbItem);
      });
    } catch (err) {
      this.log('ERROR', `Failed to load schema: ${err}`);
    }
  }

  private async runQuery(): Promise<void> {
    const sql = this.queryInput.value.trim();
    if (!sql) return;

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
      } else {
        this.statusEl.textContent = `${data.row_count} rows (${data.duration_ms}ms)`;
        this.statusEl.style.color = 'var(--success)';
        this.renderResults(data);
      }

      this.loadHistory();
      this.loadLogs();
    } catch (err) {
      this.statusEl.textContent = 'Network error';
      this.statusEl.style.color = 'var(--error)';
      this.log('ERROR', `Query failed: ${err}`);
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

  private renderError(msg: string): void {
    this.resultsTable.innerHTML = `<div class="empty-state" style="color: var(--error); padding: 40px;">⚠️ ${this.escapeHtml(msg)}</div>`;
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
