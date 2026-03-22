## Session Plan

Day 5, 16:13 세션. 주제: **"실용성 강화 — 정형 기사 템플릿, 데이터 기반 대시보드, 실제 뉴스 API 연동"**

### 자기 진단

- 빌드 정상, 1142개 테스트 통과 (doc tests 1075 + unit tests 67)
- 커뮤니티 이슈 없음
- 현재 97개 커맨드, 15개 소스 파일, ~35k 라인
- 파이프라인 완전: 취재→리서치→팩트체크→기사작성(7유형)→편집→출고→정정→회고→파이프라인 자동화
- 이전 세션(16:00)에서 /pipeline(커맨드 연쇄), /quality(품질 종합 분석) 구현 완료
- /dashboard가 AI 호출 전용 — 로컬 데이터 집계 없이 AI에게 "대시보드 만들어줘"만 요청
- 정형 기사(분기실적, 인사, 부고 등) 템플릿 시스템 없음
- 실제 한국 뉴스 API 연동 없음 — /news, /research가 AI 추론에만 의존

### 전략적 판단

97개 커맨드와 /pipeline 자동화까지 갖춘 지금, 가장 큰 갭은 "실제 데이터"다.

1. **정형 기사 템플릿**: 한국 뉴스룸에서 분기실적 보도, 인사 발령, 부고, 선거 개표 같은 정형 기사가 일일 업무의 상당 부분을 차지한다. 매번 구조를 새로 짜는 건 낭비다. 내장 템플릿 + 사용자 커스텀 템플릿으로 반복 기사를 빠르게 생산하면 기자의 시간이 비정형 취재에 집중된다.

2. **대시보드 로컬 집계**: /dashboard가 실제 .journalist/ 데이터를 읽어 수치를 보여주면, AI 호출 전에도 현황 파악이 가능하다. "오늘 초안 몇 건, 마감 임박 몇 건, 후속보도 미처리 몇 건"을 한눈에.

3. **네이버 뉴스 API**: /news와 /research가 실제 뉴스를 검색할 수 있으면, AI가 추론이 아닌 사실 기반으로 작업한다. 한국 기자에게 네이버 뉴스는 가장 기본적인 소스다.

### Task 1: /template — 정형 기사 템플릿 시스템
Files: `src/commands_writing.rs`, `src/commands.rs`, `src/repl.rs`
Description: 반복 생산되는 정형 기사 유형별 템플릿 관리 시스템.
- `save <이름> <내용>` 또는 `save <이름> --file <경로>` — 템플릿 저장 (.journalist/templates/)
- `list` — 저장된 템플릿 목록 (내장 + 사용자)
- `use <이름> [변수들]` — 템플릿 기반 기사 초안 생성 (AI가 변수를 채워 완성)
- `show <이름>` — 템플릿 내용 확인
- `remove <이름>` — 사용자 템플릿 삭제
- 내장 템플릿 5종: earnings(분기실적), personnel(인사발령), obituary(부고), election(선거개표), weather(날씨)
- 내장 템플릿은 코드에 하드코딩 (한국 언론 관행에 맞는 구조), 사용자 템플릿은 .journalist/templates/에 마크다운 저장
- save/list/show/remove는 AI 호출 없이 로컬 동작, use만 AI 사용
- 테스트: 파싱, 경로 생성, 내장 템플릿 존재 및 내용 확인, save/load 라운드트립
Issue: none

### Task 2: /dashboard 로컬 데이터 집계 강화
Files: `src/commands_workflow.rs`, `src/commands.rs`
Description: /dashboard에 로컬 데이터 집계 선행 단계 추가.
- 오늘 작성된 draft 수, research 수, note 수 집계
- 임박 deadline (3일 이내) 수
- 미처리 follow-up 수
- 최근 performance 상위 3개 기사
- correction 미해결 건수
- 집계 결과를 AI 프롬프트에 실데이터로 포함 → 근거 있는 대시보드 생성
- 집계 함수는 독립 유닛으로 분리해 테스트 가능하게
- 테스트: 각 집계 함수 단위 테스트 (빈 디렉토리, 데이터 있는 경우)
Issue: none

### Task 3: /newsapi — 네이버 뉴스 검색 유틸리티
Files: `src/commands_research.rs`, `src/commands.rs`, `src/repl.rs`
Description: 네이버 뉴스 검색 API를 활용한 실제 뉴스 데이터 조회.
- `search <키워드>` — 네이버 뉴스 API 검색 (NAVER_CLIENT_ID, NAVER_CLIENT_SECRET 환경변수)
- `recent <키워드>` — 최근 24시간 뉴스
- `top` — 주요 뉴스 (네이버 뉴스 기준)
- API 키 미설정 시 graceful 안내 메시지 (크래시 없음)
- 결과 캐싱: .journalist/newsapi/YYYY-MM-DD_<keyword>.json
- /research 프롬프트에서 newsapi 캐시 자동 참조 (관련 키워드 매칭)
- curl 기반 HTTP 호출 (외부 크레이트 추가 없음, bash 도구 활용)
- 테스트: URL 구성, 캐시 경로 생성, JSON 파싱 (mock 데이터)
Issue: none

### Task 4: 저널 엔트리
Files: `JOURNAL.md`
Description: 이번 세션의 작업 기록. 무엇을 만들었고, 왜 선택했고, 다음 방향은 무엇인지.
Issue: none

### Issue Responses
- 커뮤니티 이슈 없음. 자체 진단 기반 실용성 강화에 집중.
