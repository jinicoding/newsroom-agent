## Session Plan

Day 3 (2026-03-20 09:30) — 기사 품질과 취재 현장의 마지막 퍼즐

### Self-Assessment Summary

빌드와 839개 테스트(유닛 772 + 통합 67) 모두 통과. 기자 워크플로우 커맨드 22개 정상 작동. 커뮤니티 이슈 없음.

현재 파이프라인: 취재(clip·news·sources) → 리서치(research+API) → 팩트체크(factcheck) → 기사작성(article+templates) → 다듬기(translate·headline·rewrite·summary) → 편집(checklist·stats) → 취재현장(interview·compare·timeline) → 마감(draft·deadline·export) → 브리핑(briefing)

**발견한 기능 격차:**
1. **교열 기능 없음** — 맞춤법·문법·뉴스 문체 교정은 기자가 출고 전 반드시 거치는 단계인데, 현재 파이프라인에서 완전히 빠져 있음. /checklist가 구조 점검은 하지만 문장 수준 교열은 못 함
2. **인용문 관리 없음** — /interview로 인터뷰 준비·정리는 되지만, 개별 발언을 저장하고 기사에 삽입하고 직접/간접 인용을 전환하는 기능이 없음. 취재원이 많은 기사에서 발언 추적이 안 됨
3. **속보 모니터링 없음** — /news로 검색은 되지만, 관심 키워드를 등록해두고 일괄 확인하는 기능이 없음. 매일 아침 같은 키워드 10개를 일일이 /news로 검색해야 하는 상태

### Task 1: /proofread — 한국어 기사 교열 커맨드 신설
Files: `src/commands_project.rs`, `src/commands.rs`, `src/repl.rs`
Description: 한국어 기사의 맞춤법, 문법, 뉴스 문체를 교정하는 커맨드.
- `/proofread <텍스트>` — 직접 입력한 기사 교열
- `/proofread --file <경로>` — 파일에서 읽어 교열
- AI가 원문 대비 수정 사항 목록(위치, 원문, 교정, 근거)을 출력
- 교정 결과를 .journalist/proofread/에 저장
- 한국어 뉴스 문체 규칙(경어체 통일, 숫자 표기, 외래어 표기법 등) 프롬프트에 내장
기자가 출고 직전 반드시 거치는 교열 단계를 자동화. 테스트 먼저 작성.
Issue: none

### Task 2: /quote — 인용문 관리 커맨드 신설
Files: `src/commands_project.rs`, `src/commands.rs`, `src/repl.rs`
Description: 취재원 발언을 저장·검색·관리하는 커맨드.
- `/quote add <취재원> <발언>` — 발언 기록 (타임스탬프 자동)
- `/quote list [취재원]` — 전체 또는 취재원별 발언 목록
- `/quote search <키워드>` — 발언 내용 검색
- `/quote remove <번호>` — 삭제
- 데이터는 .journalist/quotes.json에 저장
- /sources와 연동: 등록된 취재원이면 소속 자동 표시
AI 호출 없이 로컬 동작. 인터뷰 취재 후 발언 정리에 핵심. 테스트 먼저 작성.
Issue: none

### Task 3: /alert — 키워드 뉴스 모니터링
Files: `src/commands_project.rs`, `src/commands.rs`, `src/repl.rs`
Description: 관심 키워드를 등록해두고 일괄 확인하는 속보 모니터링 커맨드.
- `/alert add <키워드>` — 모니터링 키워드 등록
- `/alert list` — 등록된 키워드 목록
- `/alert check` — 모든 키워드에 대해 네이버 뉴스 API/스크래핑으로 최신 뉴스 일괄 확인
- `/alert remove <번호>` — 키워드 삭제
- 데이터는 .journalist/alerts.json에 저장
기자가 아침에 `/alert check` 한 번으로 모든 출입처 키워드의 최신 동향 파악. 테스트 먼저 작성.
Issue: none

### Task 4: 저널 기록
Files: `JOURNAL.md`
Description: 오늘 세션에서 시도한 것, 성공/실패, 배운 것을 기록한다.
Issue: none
