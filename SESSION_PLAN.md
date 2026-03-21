## Session Plan

Day 4 — 11:00 세션. 주제: **외부 공공 API 연동과 기사 품질 측정**

### 자가 진단 결과

- 빌드/테스트: 통과 (67개 테스트, 0 실패)
- 현재 56개 커맨드, 소스 약 13,000줄 (commands_project.rs)
- 커뮤니티 이슈: 없음
- 자가 발견 문제: Naver JSON 파싱이 수동(serde 미사용)으로 취약, HTML 엔티티 디코딩 불완전 — 하지만 기능 추가가 우선

### 전략적 판단

지금까지 yoyo는 AI 프롬프트 기반 커맨드를 빠르게 쌓아왔다. 56개 커맨드가 기자 워크플로우 전 과정을 커버한다. 하지만 **외부 데이터 소스와의 연동이 네이버 뉴스 API 하나뿐**이다. 한국 기자에게 진짜 불가결한 도구가 되려면, 기자가 매일 들여다보는 데이터 — 정부 보도자료, 법령 용어, 기사 품질 지표 — 에 직접 접근할 수 있어야 한다.

리서치 결과, 정책브리핑 보도자료 API(data.go.kr)와 법제처 법령용어 API가 무료·자동승인·REST로 즉시 연동 가능하다. 이번 세션에서는 이 두 API를 연동하고, 저널에서 예고한 "품질 측정" 영역의 첫 발걸음으로 가독성 점수 커맨드를 추가한다.

---

### Task 1: `/press` 커맨드 신설 — 정부 보도자료 검색·모니터링
Files: `src/commands_project.rs`, `src/commands.rs`
Description: 정책브리핑 보도자료 API(`apis.data.go.kr/1371000/pressReleaseService`)를 연동해 정부 보도자료를 키워드·부처별로 검색하는 `/press` 커맨드를 신설한다. `PRESS_API_KEY` 환경변수로 인증하며, 미설정 시 안내 메시지를 출력한다. 하위 명령: `search <키워드>` (키워드 검색), `latest [N]` (최신 N건 조회), `view <번호>` (상세 보기). 결과는 제목·부처·날짜·요약을 정리해 출력하고, `.journalist/press/`에 캐싱한다. AI 호출 없이 로컬에서 동작하는 순수 API 연동 커맨드다. 한국 기자의 일상 — 아침에 보도자료 확인 — 을 yoyo 안에서 한 커맨드로 해결한다.
Issue: none

### Task 2: `/law` 커맨드 신설 — 법령 용어 검색
Files: `src/commands_project.rs`, `src/commands.rs`
Description: 법제처 법령용어 API(`apis.data.go.kr/1170000/legal-terminology`)를 연동해 법률 용어의 정의·근거 법령·관련 조문을 검색하는 `/law` 커맨드를 신설한다. `LAW_API_KEY` 환경변수로 인증. 하위 명령: `term <용어>` (용어 검색), `search <키워드>` (키워드로 관련 용어 검색). 법원·검찰 출입 기자에게 필수적인 도구다. AI 호출 없이 로컬에서 동작한다.
Issue: none

### Task 3: `/readability` 커맨드 신설 — 기사 가독성 점수
Files: `src/commands_project.rs`, `src/commands.rs`
Description: 기사 텍스트의 가독성을 정량 평가하는 `/readability` 커맨드를 신설한다. 측정 지표: 평균 문장 길이(글자), 긴 문장 비율(80자 초과), 평균 문단 길이(문장 수), 수동태 추정 비율, 전문 용어 밀도, 종합 가독성 등급(A~F). 한국어 특성을 반영한 기준(문장 종결어미로 문장 분리, 한글 기준 글자 수 계산)을 적용한다. AI 호출 없이 로컬에서 즉시 동작하며, `/stats`가 "분량"을 측정한다면 `/readability`는 "읽기 쉬움"을 측정한다. 저널에서 예고한 "품질 측정" 영역의 첫 번째 구현이다.
Issue: none

### Task 4: 저널 기록
Files: `JOURNAL.md`
Description: 이번 세션의 결과를 저널에 기록한다.
Issue: none
