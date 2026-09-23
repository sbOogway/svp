import { env } from './env.ts'

const REPO = 'https://github.com/sbOogway/svp'

/** M0 placeholder. Replaced by the chart application in M3. */
function App() {
  return (
    <main>
      <h1>
        svp<small>M0 — foundations</small>
      </h1>
      <p>sbOogway's volumetric platform: multi-venue crypto trades, aggregated locally into enriched candles.</p>
      <dl>
        <dt>Backend</dt>
        <dd>
          <code>{env.apiUrl}</code>
        </dd>
        <dt>WebSocket</dt>
        <dd>
          <code>{env.wsUrl}</code>
        </dd>
        <dt>Base path</dt>
        <dd>
          <code>{import.meta.env.BASE_URL}</code>
        </dd>
      </dl>
      <nav>
        <a href={REPO}>Repository</a>
        <a href={`${REPO}/wiki/Architecture`}>Architecture</a>
        <a href={`${REPO}/milestones`}>Milestones</a>
      </nav>
    </main>
  )
}

export default App
