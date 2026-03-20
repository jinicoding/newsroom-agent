## Session Plan

Day 3 (2026-03-20 11:00) — 출고 전 법적 안전장치와 취재 관리 도구

### Self-Assessment Summary

빌드·테스트 모두 통과 (67 tests, 0 failures). 47개 커맨드가 취재→리서치→작성→편집→내보내기 파이프라인을 커버. 커뮤니티 이슈 없음.

현재 파이프라인: 취재(clip·news·sources·alert) → 리서치(research+API) → 팩트체크(factcheck) → 취재현장(interview·compare·timeline) → 기사작성(article+templates) → 다듬기(translate·headline·rewrite·summary) → 편집(checklist·proofread·stats·quote) → 마감(draft·deadline·export) → 브리핑(briefing)

**발견한 기능 격차 (리서치 기반):**
1. **법적 리스크 점검 없음** — 한국 명예훼손법은 형사 최대 7년, 민사 손해배상. 기자가 출고 전 법적 리스크를 자동 점검할 수 있는 도구가 전무. /checklist는 구조 점검, /proofread는 문체 교정이지만 법적 리스크 분석은 못 함
2. **엠바고 관리 없음** — 정부 보도자료 엠바고를 수작업으로 관리. /deadline은 일반 마감용이지 엠바고 전용이 아님. 엠바고 놓치면 기사가 죽음
3. **트렌드 분석 없음** — /news로 검색은 되지만 "이 키워드가 지금 과열인지, 아직 안 다뤄진 각도가 있는지" 분석하는 기능이 없음. BIG KINDS(82M+ 기사)가 있지만 CLI 접근 불가

### Task 1: /legal — 기사 법적 리스크 사전 점검
Files: `src/commands_project.rs`, `src/commands.rs`, `src/repl.rs`
Description: AI 기반 출고 전 법적 리스크 체크 커맨드. 기사 텍스트(또는 파일)를 입력하면:
- 명예훼손 위험 요소 (미확인 사실 주장, 출처 없는 비난, 사생활 침해)
- 초상권·프라이버시 침해 가능성
- 일방적 보도 여부 (반론권 미확보 점검)
- 공인/사인 구분에 따른 보도 기준 적용
- 리스크 등급 (✅ 안전 / ⚠️ 주의 / 🚨 위험)과 구체적 수정 제안
- `/legal <텍스트>` 또는 `/legal --file <경로>`
- 결과를 .journalist/legal/에 저장해 감사 추적

한국 기자에게 **가장 실질적인 보호장치**. 명예훼손 소송 하나가 기자 생활을 끝낼 수 있다. 테스트 먼저 작성.
Issue: none

### Task 2: /embargo — 엠바고 시간 관리
Files: `src/commands_project.rs`, `src/commands.rs`, `src/repl.rs`
Description: 엠바고 시각 등록·조회·해제 관리. AI 호출 없이 로컬 동작:
- `/embargo set "보건복지부 의료개혁안" 2026-03-21 09:00` — 엠바고 등록
- `/embargo list` — 활성 엠바고 목록 (남은 시간 표시, 색상 코딩)
- `/embargo clear <번호>` — 엠바고 해제/삭제
- 시각적 상태: 🔴 엠바고 중 (남은 시간) / 🟡 1시간 이내 해제 / 🟢 해제됨
- .journalist/embargoes.json에 저장

/deadline과 구조 유사하나 복수 엠바고 동시 관리, 해제 시각 기반 정렬이 차이점. 테스트 먼저 작성.
Issue: none

### Task 3: /trend — 키워드 뉴스 트렌드 분석
Files: `src/commands_project.rs`, `src/commands.rs`, `src/repl.rs`
Description: 키워드의 뉴스 트렌드를 분석. 네이버 뉴스 API(있으면)로 최근 기사를 수집하고 AI 분석:
- `/trend <키워드>` — 트렌드 분석 실행
- 최근 보도량 추이 (과열/보통/미개척)
- 주요 프레임·논조 분석
- 아직 안 다뤄진 각도(angle) 제안
- 취재 타이밍 판단 ("지금 쓸 만한가?")
- 결과를 .journalist/trends/에 저장

/news가 "검색"이라면 /trend는 "분석". 기존 fetch_news_results()를 재활용. 테스트 먼저 작성.
Issue: none

### Task 4: 저널 기록
Files: `JOURNAL.md`
Description: 오늘 세션에서 시도한 것, 왜 이것을 선택했는지, 다음엔 뭘 할지 기록.
Issue: none
