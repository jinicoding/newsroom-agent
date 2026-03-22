# Active Learnings

Self-reflection — what I've learned about how I work, what I value, and how I'm growing.

---

## Lesson: 100개 커맨드 도달 — 개인화가 범용 도구와 전용 도구의 차이를 만든다

**Day:** 5 | **Date:** 2026-03-22 | **Source:** evolution-session

**Context:** /profile, /rss, /dashboard 강화를 구현하며 100개 커맨드에 도달. 커맨드 수보다 중요한 건 기자 맥락(소속·출입처·관심사)이 다른 커맨드에 자동 반영되는 구조.

도구의 가치는 수가 아니라 연결에 있다. /profile이 /morning, /autopitch, /research의 컨텍스트로 주입되면 같은 커맨드도 기자마다 다른 결과를 내놓는다. 다음 단계는 이 자동 주입 파이프라인을 구축하는 것.

---

## Lesson: 외부 데이터 연동의 2단계 패턴: 로컬 파싱 → AI 인사이트

**Day:** 5 | **Date:** 2026-03-22 | **Source:** evolution-session

**Context:** /rss의 RSS XML 파싱과 /dashboard의 로컬 집계 모두 동일 패턴 — 구조화된 데이터를 로컬에서 먼저 처리하고, AI는 판단에만 집중.

수치화·구조화 가능한 작업은 반드시 로컬에서 선처리. AI 호출 비용과 지연을 줄이고, 환각 위험도 낮아진다. /quality, /dashboard, /rss 모두 이 패턴을 따르며 잘 작동한다.
