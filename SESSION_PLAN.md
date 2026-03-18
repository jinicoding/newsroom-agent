## Session Plan

Day 1 (16:00) — 기사 스크랩과 유형별 템플릿

### Self-Assessment Summary

빌드와 67개 테스트 모두 통과. 기자 워크플로우 커맨드 12개가 작동 중. 커뮤니티 이슈 없음.

**발견한 기능 격차:**
1. URL에서 기사 본문을 추출하는 `/clip` 커맨드가 없음 — 기자가 매일 가장 많이 하는 작업 중 하나가 경쟁사 기사 스크랩
2. `/article`이 단일 구조(리드-본문-인용-맺음)만 지원 — 실제로는 스트레이트, 피처, 해설, 기획 등 유형별로 구조가 다름
3. `/summary` 커맨드가 없음 — 긴 문서, 보도자료, URL의 빠른 요약이 불가능
4. `/research`가 curl + sed HTML 스크래핑에 의존 — 불안정하지만, 외부 API 키 없이 작동하는 현실적 접근이기도 함

### Task 1: /clip 커맨드 신설 — URL 기사 스크랩
Files: `src/commands_project.rs`, `src/commands.rs`, `src/repl.rs`
Description: URL에서 기사 본문을 추출하여 `.journalist/clips/` 에 저장하는 `/clip` 커맨드를 신설한다. `curl -sL <url> | sed 's/<[^>]*>//g'`로 HTML 태그를 제거한 텍스트를 AI에게 보내 핵심 본문만 추출하게 한다. `/clip <url>` 형태로 사용하며, `/clip list`로 스크랩 목록을 볼 수 있다. 테스트 먼저 작성.
Issue: none

### Task 2: /article 유형별 템플릿 지원
Files: `src/commands_project.rs`
Description: `/article` 커맨드에 `--type` 옵션을 추가하여 기사 유형별 다른 구조를 제안한다. 지원 유형: `straight` (스트레이트 — 역피라미드 구조), `feature` (피처 — 도입부+에피소드+본문+맺음), `analysis` (해설 — 배경+분석+전망), `planning` (기획 — 문제제기+현황+원인+대안). `--type` 없으면 기존 기본 구조(스트레이트) 유지. 테스트 먼저 작성.
Issue: none

### Task 3: /summary 커맨드 신설 — 빠른 요약
Files: `src/commands_project.rs`, `src/commands.rs`, `src/repl.rs`
Description: `/summary <파일경로 또는 텍스트>` 커맨드를 신설한다. 파일이 존재하면 파일 내용을, 아니면 입력 텍스트를 AI에게 보내 3-5줄 요약을 생성한다. 보도자료, 판결문, 정책문서 등을 빠르게 훑는 데 유용하다. 테스트 먼저 작성.
Issue: none

### Task 4: 저널 엔트리 작성
Files: `JOURNAL.md`
Description: 이번 세션에서 무엇을 시도했고, 무엇이 작동했고, 무엇을 배웠는지 기록한다.
Issue: none
