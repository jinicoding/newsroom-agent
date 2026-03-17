## Session Plan

Day 0 자기진단: 빌드/테스트 깨끗(67 pass). 기자 4대 커맨드(/article, /research, /sources, /factcheck)가 씨앗 수준으로 존재.
커뮤니티 이슈 없음.

### Task 1: /sources 강화 — remove, edit 하위 명령 추가
Files: src/commands_project.rs
Description: 현재 /sources는 list, add, search만 지원. 기자가 취재원을 관리하려면 삭제(remove)와 수정(edit)이 필수.
- `/sources remove <번호>` — 인덱스로 취재원 삭제
- `/sources edit <번호> <필드> <값>` — 특정 필드(name/org/contact/note) 수정
- 탭 완성에 하위 명령 추가
테스트: sources_add, sources_remove, sources_edit 단위 테스트 추가
Issue: none

### Task 2: /article 결과를 파일로 저장하는 --save 옵션
Files: src/commands_project.rs
Description: /article 결과가 화면에만 출력되고 사라짐. 기자에게 초안 파일 저장은 필수 기능.
- `/article <주제>` 실행 후 AI 응답을 `.journalist/drafts/YYYY-MM-DD_<slug>.md`에 자동 저장
- `.journalist/drafts/` 디렉토리 자동 생성
- 저장 경로를 사용자에게 알림
테스트: 파일 저장 경로 생성 함수 단위 테스트
Issue: none

### Task 3: /research 결과 캐싱 — .journalist/research/ 에 저장
Files: src/commands_project.rs
Description: 리서치 결과가 대화에만 남고 파일로 남지 않음. 기자가 나중에 참고하려면 저장이 필요.
- 리서치 결과를 `.journalist/research/YYYY-MM-DD_<topic>.md`에 자동 저장
- `/research list` 하위 명령으로 기존 리서치 조회
테스트: 리서치 저장 경로 생성 함수 단위 테스트
Issue: none

### Task 4: /sources 및 /article 테스트 보강
Files: src/commands_project.rs
Description: 기자 워크플로우 커맨드에 대한 단위 테스트가 전무. 최소한 다음을 커버:
- sources JSON 파싱/직렬화 round-trip
- sources_add 입력 파싱 (인자 3개 미만 거부)
- sources_search 대소문자 무시 매칭
- article 프롬프트 생성 로직 (주제 있을 때/없을 때)
- factcheck 프롬프트 생성 로직 (빈 입력 거부)
- draft 파일 경로 생성 (슬러그화, 날짜 포함)
Issue: none
