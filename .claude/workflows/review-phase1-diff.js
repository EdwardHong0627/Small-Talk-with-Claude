export const meta = {
  name: 'review-phase1-diff',
  description: 'Adversarially review the uncommitted Phase 1 hardening diff (AppError migration, lock-poison recovery, lightbox, docs)',
  whenToUse:
    'Before committing the blog-api error-handling / rate-limiter hardening work on fix/phase0-drift-and-anon-comments.',
  phases: [
    { title: 'Review', detail: 'one worker per review dimension over the working diff' },
    { title: 'Verify', detail: 'independent refutation pass per finding' },
    { title: 'Synthesize', detail: 'rank surviving findings, flag coverage gaps' },
  ],
}

// Every spawn is pinned to the 'worker' subagent type (Sonnet, no sub-delegation).
// Do NOT add opts.model here -- 'worker' is already model-pinned by its definition.
const work = (prompt, opts = {}) => agent(prompt, { agentType: 'worker', ...opts })

const REPO = '/Users/huaichehong/Small-Talk-with-Claude'

const FINDINGS_SCHEMA = {
  type: 'object',
  properties: {
    findings: {
      type: 'array',
      items: {
        type: 'object',
        properties: {
          file: { type: 'string', description: 'repo-relative path' },
          line: { type: 'integer' },
          severity: { type: 'string', enum: ['critical', 'major', 'minor'] },
          summary: { type: 'string', description: 'one sentence: the defect' },
          failure_scenario: {
            type: 'string',
            description: 'concrete inputs/state -> wrong output, panic, or leak',
          },
        },
        required: ['file', 'line', 'severity', 'summary', 'failure_scenario'],
      },
    },
  },
  required: ['findings'],
}

const VERDICT_SCHEMA = {
  type: 'object',
  properties: {
    refuted: { type: 'boolean' },
    reasoning: { type: 'string' },
  },
  required: ['refuted', 'reasoning'],
}

const BASE = `Repo: ${REPO}. Branch fix/phase0-drift-and-anon-comments.
Review ONLY the uncommitted working-tree diff (\`git diff\` plus untracked
blog-api/src/error.rs and blog-api/deploy/backup-blog-api.sh). Committed code is
out of scope except as context for whether the diff broke it.

Baseline (already verified by the orchestrator -- do not re-run to confirm, only
to test a specific hypothesis): blog-api 29 tests pass, mdpub 80 pass, clippy
reports 1 warning (large Err variant, src/routes/mod.rs:26).

Report ONLY defects you can tie to a concrete failure scenario. A style opinion,
a "consider extracting", or a restatement of what the code does is NOT a finding.
If the dimension is clean, return an empty findings array -- that is a valid and
useful result. Do not pad.`

const DIMENSIONS = [
  {
    key: 'error-semantics',
    prompt: `${BASE}

DIMENSION: AppError migration correctness.
The diff converted handlers from .unwrap() to Result<Response, AppError> across
admin.rs, comments.rs, contact.rs, reactions.rs.
Check specifically:
- Did any handler's success-path status code, body shape, or headers change?
- Does any path that previously returned a 4xx (e.g. bad_request / validate_len)
  now collapse into AppError's generic 500?
- Is any rusqlite error that is semantically "not found" or "conflict" now a 500
  where the client needs to distinguish it?
- Does '?' on a rusqlite call sit anywhere that previously had deliberate
  error recovery that is now short-circuited?
Cross-check against blog-api/tests/ to see which behaviors are actually pinned.`,
  },
  {
    key: 'locking',
    prompt: `${BASE}

DIMENSION: concurrency and lock discipline.
The diff added AppState::conn() (blog-api/src/lib.rs) doing
self.conn.lock().unwrap_or_else(|e| e.into_inner()) and the same poison recovery
in ratelimit.rs:87.
Check specifically:
- Is a std::sync::MutexGuard held across an .await point in any async handler?
  If so, name the exact handler and await.
- Poison recovery via into_inner() means a panic mid-transaction leaves the
  Connection observable in a half-mutated state to the next caller. Is there a
  concrete write path where that yields corrupt or partial data rather than a
  clean failure?
- Any lock ordering that could deadlock (conn + ratelimit windows held together)?
- Does ratelimit's windows map still grow unbounded (no eviction)? If so, give
  the memory-growth scenario.`,
  },
  {
    key: 'security',
    prompt: `${BASE}

DIMENSION: security and information disclosure.
Check specifically:
- Does AppError's IntoResponse leak internal detail (SQL text, paths, schema) in
  the response body, vs only via tracing? Read blog-api/src/error.rs closely.
- Does tracing::error! log any PII or secret (emails from contact.rs, IPs)?
- Do the admin.rs changes preserve the auth middleware contract -- can any
  now-Result-returning admin handler be reached unauthenticated?
- Review blog-api/deploy/backup-blog-api.sh (untracked, new): file permissions on
  the backup artifact, secrets in argv or env, rm/overwrite that could destroy the
  live DB, missing set -euo pipefail, unquoted expansions.`,
  },
  {
    key: 'frontend-docs',
    prompt: `${BASE}

DIMENSION: frontend and docs.
- blog/static/js/lightbox.js (~124 lines changed): event-listener leaks on repeat
  open/close, focus trap / Escape handling, any innerHTML assignment from
  non-literal data (the codebase rule is textContent only -- flag any violation as
  critical), behavior if an image fails to load.
- docs/server-setup.md: does any documented command contradict what the code now
  does, or instruct the user to run something destructive against the live host at
  172.104.62.92? Flag stale instructions as findings.`,
  },
]

phase('Review')
log(`Reviewing working diff across ${DIMENSIONS.length} dimensions with 'worker' agents`)

// pipeline: each dimension's findings go straight to verification as soon as that
// dimension finishes -- no barrier, so a slow dimension never blocks a fast one's
// verify stage.
const reviewed = await pipeline(
  DIMENSIONS,
  (d) => work(d.prompt, { label: `review:${d.key}`, phase: 'Review', schema: FINDINGS_SCHEMA }),
  (result, d) => {
    if (!result || !result.findings.length) {
      log(`${d.key}: clean`)
      return []
    }
    log(`${d.key}: ${result.findings.length} candidate finding(s) -> verify`)
    // Each finding is refuted independently by 2 skeptics; it survives only if
    // both fail to refute it.
    return parallel(
      result.findings.map((f) => () =>
        parallel(
          [0, 1].map((i) => () =>
            work(
              `${BASE}

Adversarially REFUTE this claimed defect. Your default is refuted=true; only set
refuted=false if you can trace the exact failing path in the real code and it
holds up.

  file: ${f.file}:${f.line}
  claim: ${f.summary}
  claimed failure: ${f.failure_scenario}

Read the actual file. Verify the claimed code path exists as described and that
no guard, caller, type, or test upstream already prevents it. ${
                i === 1
                  ? 'Lens: check whether an existing test in blog-api/tests/ or an mdpub test already pins this behavior, proving the claim wrong.'
                  : 'Lens: check whether the failure is reachable at all from a real HTTP request or CLI invocation.'
              }`,
              { label: `refute:${f.file.split('/').pop()}:${f.line}#${i}`, phase: 'Verify', schema: VERDICT_SCHEMA },
            ),
          ),
        ).then((votes) => {
          const live = votes.filter(Boolean)
          // Unanimous non-refutation required; a dead/errored skeptic never
          // counts as agreement.
          const survives = live.length === 2 && live.every((v) => !v.refuted)
          return survives ? { ...f, dimension: d.key, defense: live.map((v) => v.reasoning) } : null
        }),
      ),
    )
  },
)

const confirmed = reviewed.flat(2).filter(Boolean)
const RANK = { critical: 0, major: 1, minor: 2 }
confirmed.sort((a, b) => RANK[a.severity] - RANK[b.severity])

log(`${confirmed.length} finding(s) survived refutation`)

phase('Synthesize')
const gaps = await work(
  `${BASE}

A review just ran over this diff on 4 dimensions: ${DIMENSIONS.map((d) => d.key).join(', ')}.
It confirmed these findings after adversarial verification:
${confirmed.length ? JSON.stringify(confirmed.map((f) => ({ file: f.file, line: f.line, severity: f.severity, summary: f.summary })), null, 2) : '(none)'}

Your job is the completeness critic. Do NOT re-review the code for new bugs.
Answer only: what did this review structurally MISS? Consider -- a changed file no
dimension covered, a behavior with no test pinning it, a claim accepted without
anyone reading the relevant test, an interaction between two dimensions nobody
looked at. Be concrete and brief: a bulleted list, max 6 items.`,
  { label: 'critic:coverage', phase: 'Synthesize' },
)

return {
  confirmed,
  coverage_gaps: gaps,
  known_baseline: {
    blog_api_tests: '29 passed',
    mdpub_tests: '80 passed',
    clippy: '1 warning: large Err variant at src/routes/mod.rs:26',
  },
}
