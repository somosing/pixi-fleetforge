<script lang="ts">
    import { onMount } from 'svelte';

    type RobotState = {
        robot_id: string;
        active_version: string | null;
        previous_version: string | null;
        pending_version: string | null;
        phase: string;
        detail: string;
    };
    type Observation = { state: RobotState; last_seen_ms: number; endpoint: string | null };
    type Release = { version: string; sha256: string; artifact_kind: string };
    type FleetSnapshot = {
        desired: Record<string, Release>;
        observed: Record<string, Observation>;
    };

    let hubAddress = 'http://127.0.0.1:4500';
    let accessToken = '';
    let fleet: FleetSnapshot = { desired: {}, observed: {} };
    let statusMessage = 'Enter your local hub token to connect';
    let selectedRobots = '';
    let releaseVersion = '';
    let artifactDigest = '';
    let artifactKind = 'executable';
    let lastUpdated = 0;

    $: robotRows = Object.entries(fleet.observed).sort(([left], [right]) => left.localeCompare(right));

    async function requestHub(endpoint: string, method = 'GET', payload?: unknown) {
        if (!accessToken) {
            throw new Error('Fleet access token is required');
        }
        const response = await fetch(`${hubAddress.replace(/\/$/, '')}${endpoint}`, {
            method,
            headers: {
                Authorization: `Bearer ${accessToken}`,
                ...(payload ? { 'Content-Type': 'application/json' } : {})
            },
            ...(payload ? { body: JSON.stringify(payload) } : {})
        });
        if (!response.ok) {
            throw new Error(`${response.status} ${await response.text()}`);
        }
        return response;
    }

    async function refreshFleet() {
        if (!accessToken) return;
        try {
            const response = await requestHub('/api/fleet');
            fleet = (await response.json()) as FleetSnapshot;
            statusMessage = 'Connected to FleetForge hub';
            lastUpdated = Date.now();
        } catch (networkError) {
            statusMessage = networkError instanceof Error ? networkError.message : 'Connection failed';
        }
    }

    async function submitRollout() {
        try {
            const robotIds = selectedRobots.split(',').map((robotId) => robotId.trim()).filter(Boolean);
            if (!robotIds.length || !releaseVersion || !/^[0-9a-f]{64}$/i.test(artifactDigest)) {
                statusMessage = 'Specify robot IDs, a release version and a 64-character SHA-256 digest';
                return;
            }
            await requestHub('/api/desired', 'POST', {
                robot_ids: robotIds,
                release: {
                    version: releaseVersion,
                    sha256: artifactDigest.toLowerCase(),
                    artifact_kind: artifactKind,
                    health_timeout_ms: 10000
                }
            });
            statusMessage = `Desired release ${releaseVersion} sent to ${robotIds.length} robot(s)`;
            await refreshFleet();
        } catch (rolloutError) {
            statusMessage = rolloutError instanceof Error ? rolloutError.message : 'Rollout failed';
        }
    }

    function isOnline(observation: Observation) {
        return lastUpdated - observation.last_seen_ms < 15000;
    }

    onMount(() => {
        const refreshTimer = setInterval(() => { void refreshFleet(); }, 4000);
        return () => clearInterval(refreshTimer);
    });
</script>

<svelte:head>
    <title>FleetForge — Robot Fleet Operations</title>
    <meta name="description" content="Local robotics release operations dashboard" />
</svelte:head>

<div class="shell">
    <header>
        <div class="brand"><span class="mark">FF</span><div><h1>FleetForge</h1><small>ROBOT FLEET OPERATIONS · LOCAL PROTOTYPE</small></div></div>
        <span class="version">v0.2 · Development</span>
    </header>
    <main>
        <section class="connection">
            <div class="section-title"><h2>Control plane</h2><span>{statusMessage}</span></div>
            <div class="connect-controls">
                <label>Hub address<input bind:value={hubAddress} aria-label="Hub address" /></label>
                <label>Access token<input type="password" bind:value={accessToken} aria-label="Access token" autocomplete="off" /></label>
                <button onclick={refreshFleet}>Connect / Refresh</button>
            </div>
            <p class="hint">Token stays in page memory. The hub must run on your local machine and be started with the same FLEETFORGE_TOKEN.</p>
        </section>

        <section class="fleet-section">
            <div class="section-title"><h2>Robots</h2><span>{robotRows.length} observed · Last refresh {lastUpdated ? new Date(lastUpdated).toLocaleTimeString() : 'never'}</span></div>
            <div class="fleet-grid">
                {#each robotRows as [robotId, observation] (robotId)}
                    <article class="robot-card">
                        <div class="card-header"><strong>{robotId}</strong><span class:online={isOnline(observation)} class="indicator">{isOnline(observation) ? 'Online' : 'Stale'}</span></div>
                        <p class="release">{observation.state.active_version ?? 'No release'}</p>
                        <div class="state-row"><span>Phase</span><strong>{observation.state.phase.replaceAll('_', ' ')}</strong></div>
                        <div class="state-row"><span>Desired</span><strong>{fleet.desired[robotId]?.version ?? '—'}</strong></div>
                        <div class="state-row"><span>Previous</span><strong>{observation.state.previous_version ?? '—'}</strong></div>
                        <p class="detail">{observation.state.detail}</p>
                    </article>
                {:else}
                    <p class="empty">No robot heartbeats yet. Start the hub and fleet agents.</p>
                {/each}
            </div>
        </section>

        <section class="rollout-section">
            <div class="section-title"><h2>Assign desired release</h2><span>Artifact must already be uploaded to the hub</span></div>
            <form onsubmit={(event) => { event.preventDefault(); void submitRollout(); }}>
                <label>Robot IDs (comma-separated)<input bind:value={selectedRobots} placeholder="robot-1,robot-2" /></label>
                <label>Version<input bind:value={releaseVersion} placeholder="2.0.0" /></label>
                <label>Artifact SHA-256<input bind:value={artifactDigest} placeholder="64 hexadecimal characters" /></label>
                <label>Artifact type<select bind:value={artifactKind}><option value="executable">Executable</option><option value="pixi_archive">Pixi archive</option></select></label>
                <button type="submit">Assign release</button>
            </form>
            <p class="hint">This interface is for local development only. Deployment authorization, operator roles and secure remote transport are not production hardened.</p>
        </section>
    </main>
    <footer>FleetForge · failure-aware deployment research · <a href="https://github.com/somosing/pixi-fleetforge">GitHub</a></footer>
</div>

<style>
    :global(*){box-sizing:border-box}
    :global(body){margin:0;background:#0c111b;color:#e5ecf7;font-family:system-ui,-apple-system,BlinkMacSystemFont,'Segoe UI',sans-serif}
    :global(input),:global(select){outline-offset:3px}
    .shell{max-width:1180px;margin:auto;padding:0 24px}
    header{display:flex;align-items:center;justify-content:space-between;padding:27px 0;border-bottom:1px solid #253044}
    .brand{display:flex;align-items:center;gap:14px}.mark{display:grid;place-items:center;width:48px;height:48px;border:1px solid #4faed3;color:#80d5ef;border-radius:10px;font-weight:800}
    h1{font-size:24px;letter-spacing:-.035em;margin:0}small{font-size:10px;color:#8e9db5;letter-spacing:.13em}
    .version{font-size:12px;color:#a4b2c4}main{display:grid;gap:25px;padding:28px 0}
    section{background:#151e2b;border:1px solid #29384d;border-radius:14px;padding:23px}
    .section-title{display:flex;align-items:baseline;justify-content:space-between;gap:12px;margin-bottom:22px}
    .section-title span,.hint{color:#9eabc0;font-size:12px}h2{font-size:17px;font-weight:650;margin:0}
    .connect-controls,form{display:grid;grid-template-columns:1fr 1fr auto;align-items:end;gap:13px}
    label{display:grid;gap:7px;color:#aebbd1;font-size:12px}input,select{background:#0c1420;border:1px solid #374963;border-radius:7px;padding:12px;color:#eff5ff;min-width:0;width:100%}
    button{background:#53b5d6;border:0;border-radius:7px;padding:13px 17px;color:#061422;font-weight:700;cursor:pointer}button:hover{background:#8cdef2}
    .hint{margin:17px 0 0;line-height:1.6}.fleet-grid{display:grid;grid-template-columns:repeat(auto-fill,minmax(255px,1fr));gap:13px}
    .robot-card{background:#101824;border:1px solid #2d3a4e;border-radius:10px;padding:18px}
    .card-header,.state-row{display:flex;justify-content:space-between;gap:12px}.card-header{font-size:14px}
    .indicator{font-size:11px;color:#f2bd82}.indicator.online{color:#71e0b7}.release{font-size:26px;font-weight:750;letter-spacing:-.04em;margin:24px 0}
    .state-row{color:#94a5ba;font-size:12px;padding:7px 0;border-bottom:1px solid #253146}.state-row strong{color:#dce6f4;font-weight:500;text-transform:capitalize}
    .detail{font-size:12px;color:#a5b6ca;margin:18px 0 0;line-height:1.6}.empty{color:#99abc3;font-size:13px}
    form{grid-template-columns:repeat(2, minmax(0,1fr))}form button{grid-column:2;justify-self:end}
    footer{padding:20px 0 36px;border-top:1px solid #253044;font-size:12px;color:#73839d}a{color:#8ed1f3}
    @media(max-width:680px){.connect-controls,form{grid-template-columns:1fr}form button{grid-column:1;justify-self:stretch}.section-title{flex-direction:column}header{align-items:flex-start}.shell{padding:0 14px}}
</style>
