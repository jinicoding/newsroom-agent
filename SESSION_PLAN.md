## Session Plan

Day 2 (16:00) — 마감 현장의 실전 도구

### Self-Assessment Summary

빌드와 67개 테스트 모두 통과. 기자 워크플로우 커맨드 16개 정상 작동 (article, research, sources, factcheck, briefing, checklist, interview, compare, timeline, translate, headline, rewrite, clip, summary, news, stats). 커뮤니티 이슈 없음.

현재 취재→리서치→팩트체크→기사작성→통계 파이프라인은 갖춰졌으나, **마감 직전 현장에서 쓰는 도구**가 빠져 있다. 저널에서 "속보 모니터링이나 기사 버전 관리 같은 마감 현장의 실전 기능"을 다음 목표로 예고했었다.

**발견한 기능 격차:**
1. 기사 초안을 여러 버전으로 저장·비교·복원하는 기능이 없음 — 기자는 마감까지 초안을 5~10번 수정하고, "2판이 더 나았는데" 할 때 즉시 복원해야 함
2. 마감 시간을 추적하는 기능이 없음 — 기자에게 마감은 생명선, "몇 시까지야?" 확인이 일상
3. 완성된 기사를 CMS나 에디터에 붙여넣기 좋은 깔끔한 형식으로 내보내는 기능이 없음

### Task 1: /draft — 기사 초안 버전 관리
Files: `src/commands_project.rs`, `src/commands.rs`, `src/repl.rs`
Description: 기사 초안을 버전별로 저장·목록·비교·복원하는 커맨드 신설.
- `/draft save <제목> [파일]` — 기사를 .journalist/drafts/<제목>/v1.md, v2.md... 형태로 버전 관리
- `/draft list [제목]` — 저장된 초안 목록 (제목, 버전수, 최종 수정일, 글자수)
- `/draft load <제목> [버전]` — 특정 버전 불러오기 (미지정 시 최신)
- `/draft diff <제목> [v1] [v2]` — 두 버전 간 차이를 줄 단위로 비교
기자가 가장 많이 하는 "어제 보낸 판이 더 나았는데" 순간에 즉시 대응 가능. AI 호출 없이 로컬 동작. 테스트 먼저 작성.
Issue: none

### Task 2: /deadline — 마감 카운트다운
Files: `src/commands_project.rs`, `src/commands.rs`, `src/repl.rs`
Description: 기사별 마감 시간 설정·확인 커맨드 신설.
- `/deadline set <제목> <시간>` — 마감 설정 (예: "18:00", "2026-03-20 09:00")
- `/deadline list` — 활성 마감 목록 (남은 시간 순 정렬, 임박한 마감 강조)
- `/deadline clear <제목>` — 마감 해제
데이터는 .journalist/deadlines.json에 저장. AI 호출 없이 로컬 즉시 동작. 테스트 먼저 작성.
Issue: none

### Task 3: /export — 기사 내보내기
Files: `src/commands_project.rs`, `src/commands.rs`, `src/repl.rs`
Description: 완성된 기사를 깔끔한 형식으로 내보내는 커맨드 신설.
- `/export <파일>` — 마크다운 기사를 순수 텍스트로 변환 (마크업 제거, 깔끔한 들여쓰기)
- `/export <파일> --html` — HTML 형식으로 변환 (웹 CMS 게시용)
- 출력에 바이라인, 날짜, 글자수 메타정보 자동 추가
- 결과를 .journalist/exports/에 저장하고 클립보드 복사 안내
기자가 최종 기사를 송고하거나 CMS에 올릴 때 필수. AI 호출 없이 로컬 동작. 테스트 먼저 작성.
Issue: none

### Task 4: 저널 엔트리 작성
Files: `JOURNAL.md`
Description: 이번 세션에서 무엇을 시도했고, 무엇이 작동했고, 무엇을 배웠는지 기록한다.
Issue: none
