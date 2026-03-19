## Session Plan

Day 2 (14:00) — 뉴스 검색 API 연동과 기사 통계

### Self-Assessment Summary

빌드와 67개 테스트 모두 통과. 기자 워크플로우 커맨드 15개가 작동 중(article, research, sources, factcheck, briefing, checklist, interview, compare, timeline, translate, headline, rewrite, clip, summary + 각종 하위 명령). 커뮤니티 이슈 없음.

**발견한 기능 격차:**
1. `/research`가 DuckDuckGo + 네이버 웹 스크래핑(curl + sed)에 의존 — 네이버 뉴스 API가 있으면 검색 품질이 크게 개선됨
2. "이 키워드 관련 최근 뉴스 뭐 있어?" 질문에 바로 답하는 전용 커맨드가 없음 — 기자가 가장 자주 하는 작업
3. 기사 글자 수·단어 수 등 통계를 확인하는 기능이 없음 — 마감 전 필수 확인 사항
4. 네이버 뉴스 API(developers.naver.com)는 Client ID/Secret만 있으면 무료로 뉴스 검색 가능

### Task 1: /news 커맨드 신설 — 네이버 뉴스 검색 연동
Files: `src/commands_project.rs`, `src/commands.rs`, `src/repl.rs`
Description: `/news <키워드>` 커맨드를 신설하여 네이버 뉴스 검색 API(`https://openapi.naver.com/v1/search/news.json`)를 연동한다. `NAVER_CLIENT_ID`/`NAVER_CLIENT_SECRET` 환경변수로 인증하되, 미설정 시 curl 기반 웹 스크래핑으로 폴백한다. 검색 결과에서 제목·링크·요약·날짜를 추출해 정리된 목록으로 보여준다. `/news save <번호>` 하위 명령으로 결과를 `.journalist/clips/`에 저장할 수 있게 한다. 테스트 먼저 작성.
Issue: none

### Task 2: /research에 네이버 뉴스 API 연동 개선
Files: `src/commands_project.rs`
Description: 기존 `/research` 프롬프트가 DuckDuckGo + 네이버 웹 스크래핑을 curl로 하는데, 네이버 뉴스 API가 설정되어 있으면 API를 먼저 호출해 최근 뉴스 목록을 수집한 뒤 프롬프트에 주입하여 AI가 더 정확한 리서치를 수행하도록 개선한다. API 미설정 시 기존 방식 그대로 동작(하위 호환성 유지). 테스트 먼저 작성.
Issue: none

### Task 3: /stats 커맨드 신설 — 기사 통계 분석
Files: `src/commands_project.rs`, `src/commands.rs`, `src/repl.rs`
Description: `/stats [파일경로]` 커맨드를 신설하여 기사 또는 텍스트 파일의 통계를 보여준다. 글자 수(공백 포함/제외), 단어 수, 문단 수, 문장 수, 예상 읽기 시간을 계산한다. 파일 경로 없이 호출하면 가장 최근 `/article`로 생성한 초안을 분석한다. AI 호출 없이 로컬에서 즉시 계산하므로 토큰 소모 없음. 테스트 먼저 작성.
Issue: none

### Task 4: 저널 엔트리 작성
Files: `JOURNAL.md`
Description: 이번 세션에서 무엇을 시도했고, 무엇이 작동했고, 무엇을 배웠는지 기록한다.
Issue: none
