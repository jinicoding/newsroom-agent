## Session Plan

Day 3 (2026-03-20 14:00) — 출고 이후 워크플로우와 데이터 저널리즘

### Self-Assessment Summary

빌드·테스트 모두 통과 (67 tests, 0 failures). 47개 커맨드가 취재→출고 파이프라인을 촘촘하게 커버. 커뮤니티 이슈 없음.

현재 파이프라인: 취재(clip·news·sources·alert) → 리서치(research+API) → 트렌드분석(trend) → 팩트체크(factcheck) → 취재현장(interview·compare·timeline) → 기사작성(article+templates) → 다듬기(translate·headline·rewrite·summary) → 편집(checklist·proofread·stats·quote) → 법적점검(legal) → 마감(draft·deadline·embargo·export) → 브리핑(briefing)

**발견한 기능 격차:**
1. **출고 이후 관리 전무** — 기사를 쓰고 내보내면 끝. 출고된 기사를 체계적으로 보관하고 검색하는 아카이브가 없다. 기자는 과거 기사를 수시로 참조한다 ("지난달 그 기사 어디 갔지?"). 아카이브 없이는 파일 시스템을 뒤져야 한다.
2. **데이터 저널리즘 지원 없음** — 데이터 저널리즘이 한국 언론에서 빠르게 성장 중이나, 숫자 데이터를 분석하고 기사 앵글을 찾는 도구가 없다. /stats는 글자 수 세기일 뿐이다.
3. **후속 보도 추적 없음** — 1보 출고 후 2보를 까먹는 건 데스크에서 가장 흔한 사고. 후속 보도 계획을 등록하고 알림받는 시스템이 없다.

### Task 1: /archive — 출고 기사 아카이브 시스템
Files: `src/commands_project.rs`, `src/commands.rs`
Description: 출고된 기사를 체계적으로 아카이브하고 과거 기사를 검색하는 커맨드. AI 호출 없이 로컬 동작.
- `/archive save <제목> [--section 경제] [--type 스트레이트] [--tags 반도체,삼성]` + 파이프 또는 파일 경로로 본문 입력
- `/archive list [--section 경제] [--recent 10]` — 아카이브 목록 (날짜·제목·섹션 표시)
- `/archive search <키워드>` — 제목·본문·태그에서 키워드 검색
- `/archive view <번호>` — 기사 전문 열람
- .journalist/archive/에 JSON 메타데이터(날짜, 제목, 섹션, 유형, 키워드 태그) + 텍스트 파일로 저장
- 기자가 "지난달에 쓴 반도체 기사 뭐였지?"에 즉시 답하는 도구
Issue: none

### Task 2: /data — 데이터 저널리즘 분석 지원
Files: `src/commands_project.rs`, `src/commands.rs`
Description: CSV 파일이나 데이터를 AI에게 넘겨 분석하는 커맨드.
- `/data analyze <파일경로>` — AI가 데이터를 읽고 핵심 수치, 추세, 이상치 식별, 기사 앵글 제안
- `/data summarize <파일경로>` — 로컬에서 기본 통계 (행/열 수, 수치 칼럼 통계, 결측치)
- `/data compare <파일1> <파일2>` — 두 데이터셋의 차이 분석
- .journalist/data/에 분석 결과 저장
- 데이터 저널리즘의 첫 단계 — "숫자 더미에서 기사 앵글 찾기"를 보조
Issue: none

### Task 3: /follow — 후속 보도 추적 시스템
Files: `src/commands_project.rs`, `src/commands.rs`
Description: 기사 출고 후 후속 보도 계획을 관리하는 커맨드. AI 호출 없이 로컬 동작.
- `/follow add <주제> [--due 2026-03-25]` — 후속 보도 등록
- `/follow list` — 활성 후속 보도 목록 (마감일 기준 정렬, 색상 코딩)
- `/follow done <번호>` — 완료 처리
- `/follow remind` — 임박한 후속 보도 알림 (3일 이내)
- .journalist/followups.json에 저장
- 데스크가 "그 건 후속 잡았어?"라고 물을 때 즉시 답하는 도구
Issue: none

### Task 4: 저널 기록
Files: `JOURNAL.md`
Description: 오늘 세션에서 시도한 것, 왜 이것을 선택했는지, 다음엔 뭘 할지 기록.
Issue: none
