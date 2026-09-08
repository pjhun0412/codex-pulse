# 작업표시줄 위젯이 전체화면 위에 남는 문제

- 날짜: 2026-09-08
- 상태: 표시·입력 후속 수정 및 폭 축소 적용, 최신본 전체화면 재검증 남음

## 증상과 원인

사용자가 전체화면 및 원격 접속 화면에서도 작업표시줄 위젯이 위에 남는 현상을 보고했다.
`src-tauri/src/native_widget.rs`는 독립 `WS_POPUP` 창에 `WS_EX_TOPMOST`를 지정했고,
위치 타이머와 `WM_WINDOWPOSCHANGING`에서도 `HWND_TOPMOST`를 강제했다.
`GWLP_HWNDPARENT`는 팝업의 소유자만 Explorer로 지정하므로 작업표시줄의 실제 자식 창이 아니었다.

## 변경

- `WS_POPUP`을 `WS_CHILD`로 전환한 뒤 `SetParent`로 작업표시줄에 연결한다.
- 위젯의 최상위 스타일과 최상위 재지정, 숨김·최소화 방해 처리를 제거한다.
- 작업표시줄 화면 좌표를 부모 클라이언트 좌표로 변환한다. 상세 팝업용 경계는 화면 좌표를 유지한다.
- 작업표시줄을 찾지 못하거나 연결에 실패하면 위젯을 숨기고 재시도한다.
- 부모가 숨겨져서 `IsWindowVisible`이 거짓이 된 경우 표시를 강제하지 않는다.
- Explorer 재시작으로 자식 창이 파괴돼도 살아 있는 스레드 타이머에서 창을 다시 생성한다.
- 자유 오버레이 및 별도 상세 팝업의 동작은 기존대로 유지한다.
- 실제 도킹 자식 창은 `Shell_TrayWnd` 바로 아래에 둔다. `SetParent` 이후 layered 스타일을 재설정하고 `UpdateLayeredWindow(ULW_OPAQUE)`로 비트맵을 합성한다. 창 핸들과 GDI 성공 여부만으로 화면 표시를 판단하지 않는다.
- `app.manifest`에 Windows 10/11 호환성 선언을 넣어 layered 자식 창을 지원한다. Tauri의 Common Controls v6 선언은 유지한다.
- `WM_RBUTTONUP`을 직접 처리해 작업표시줄 자식 창에서도 우클릭 메뉴를 연다.
- 배경색은 화면 샘플 지점이 실제 작업표시줄 계층에 속할 때만 갱신한다. 메뉴·상세창이 열려 있거나 다른 창이 덮으면 직전 색을 유지한다.
- 네이티브 위젯 폭을 306에서 248 논리 픽셀로 줄인다. 데이터와 글자 크기는 유지한다.
- `main.rs`의 Windows GUI 서브시스템 선언을 디버그 빌드에도 적용해 CMD 창이 따라 뜨지 않도록 한다.

`SetParent`는 Windows API이지만 Explorer 작업표시줄 내부 삽입은 공식 확장 계약이 아니다.
Windows 버전, 배율, Explorer 변경에 따른 실제 동작을 검증해야 한다.

## 검증

### 후속 사용자 제보로 확인한 한계와 최종 검증

최초 테스트는 창 계층, 표시 플래그와 메시지 처리만 검사했다. 사용자의 화면에서 위젯이 보이지 않았으므로
이를 정상 표시로 보고한 결론은 잘못됐다. 이후 실제 화면 캡처와 OS 마우스 입력을 검사 항목에 추가했다.

- 자식 창의 GDI 그리기는 성공하고 클릭 판정도 가능했지만 실제 합성 화면에는 나오지 않았다. 재부착 후 layered 표면을 재생성하자 실제 글자가 표시됐다.
- XAML 합성 브리지 아래에 연결한 시도는 표시와 입력을 동시에 충족하지 못했다. 최종본은 작업표시줄 바로 아래의 layered 자식 창에 `UpdateLayeredWindow`를 사용한다.
- 실제 좌클릭으로 상세창 열기·닫기, 우클릭 메뉴 열기·닫기와 화면의 민트 표시선 픽셀 검사가 통과했다. 후속 사용자 메뉴 조작 로그에서도 반응을 확인했다.
- 폭 축소와 색상 캐시 적용 후 캡처 `qa/taskbar-compact.png`에서 248px 위젯과 표시 내용을 확인했다. `qa/` 캡처는 로컬 검증 자료로 Git 추적 대상이 아니다.
- 최종 실행 파일의 PE Subsystem 값 2(Windows GUI), Windows 호환성 manifest 포함을 확인했다. 따라서 디버그 위젯도 자체 콘솔을 만들지 않는다.
- 최신 네이티브 빌드 및 Rust 테스트 7개 통과. 아래 초기 전체화면 성공은 렌더러 변경 전 결과이며 최신본 전체화면 통과로 간주하지 않는다.
- 최종 통합 검사 도중 사용자 클릭·배치 모드 전환과 자동 검사가 겹쳤다. 작업표시줄 모드가 아닌 상태에서 검사도 실행돼 후속 자동 입력을 중단했다. 최신본 전체화면과 메뉴 표시 중 색상 안정성의 자동 검사는 완료하지 못했다.

### 초기 검사 기록 (렌더러 변경 전)

- `npm run build:frontend`: 타입 검사 및 프런트 빌드 통과. 로컬 미리보기 로드 확인.
- `cargo check`: 통과. 기존 `taskbar.rs` 미사용 함수 경고 6개가 남아 있다.
- `cargo test --features tauri/custom-protocol --lib`: 7개 통과. 새 Windows 테스트 2개는 실제 숨겨진 창으로 부모 관계, 최상위 속성 부재, 좌표 변환, 부모 숨김·파괴의 전파 및 연결 실패 복원을 확인한다.
- `npm run build:native`: 통과. 실행 파일은 `src-tauri/target/debug/codex-pulse.exe`.
- 2026-09-08 21:43 KST 수정본 실행 로그에서 작업표시줄 자식 연결 성공과 `style=0x54000000` (`WS_CHILD`, `WS_VISIBLE` 포함), `ex_style=0x8000080` (`WS_EX_TOPMOST` 없음)을 확인했다. 배치 경계는 `(8, 1032, 306, 48)`이다.
- 변경 파일 `rustfmt --check` 및 `git diff --check` 통과. 프로젝트 전체 `cargo fmt --check`는 변경하지 않은 `direct_text.rs`, `taskbar.rs`의 기존 포맷 차이 때문에 통과하지 않는다.
- 수정본을 실행했으며 기존 설치 파일은 교체하지 않았다. 자동 시작 또는 설치본 바로가기는 기존 버전을 실행할 수 있다.
- 최초 Windows UI 검증은 도구의 자식 창 식별 제한과 사용자 Escape 중단으로 끝났다. 후속 테스트 요청에 따라 `examples/taskbar_smoke.rs`로 실행 중인 수정본을 대상으로 네이티브 통합 테스트를 수행했고 통과했다.
- 통합 테스트: `GA_PARENT`로 실제 Explorer 부모 확인, `WS_CHILD` 및 비최상위 확인, 위젯 위치의 `WindowFromPoint` 판정, 클릭 메시지 처리에 따른 상세창 열기·닫기, 상세창 인접 배치를 확인했다. 이는 물리 마우스 입력 또는 글꼴·색상 시각 검사를 대신하지 않는다.
- 모니터 전체를 덮는 **최상위가 아닌** 테스트 창을 전경에 띄워 위젯 위치가 3초간 연속으로 가려지는지 검사했다. 종료 후 위젯 표시 및 클릭 위치 판정 복원도 통과했다.
- 최초 전체화면 테스트는 Windows 전경 전환 지연으로 전제 조건을 충족하지 못했다. 테스트가 실제 전경 상태를 기다린 뒤 검사하도록 보완한 재실행은 종료 코드 0으로 통과했다.
- 실제 원격 데스크톱 전체화면, 게임의 독점 전체화면, 다중 모니터·배율 변경, Explorer 재시작 후 복구는 각각 별도 확인이 필요하다. 부모 파괴 전파 테스트는 Explorer 재시작 전체 과정의 검증을 대신하지 않는다.

## 통합 테스트 재실행

수정본을 작업표시줄 모드로 실행하고 상세창을 닫은 상태에서 해당 프로세스의 PID를 사용한다.

```powershell
cargo run --manifest-path src-tauri/Cargo.toml --target-dir "$env:TEMP/codex-pulse-cargo-target-v2" --features tauri/custom-protocol --example taskbar_smoke -j 1 -- <PID>
```

테스트는 대상 PID와 위젯 클래스가 일치하는 창만 선택한다. Explorer나 위젯 프로세스를 종료하지 않는다.
현재 테스트는 실제 OS 마우스 입력을 주입하고 종료 시 원래 커서 위치로 복원한다. 검사 중 수동 클릭이나 배치 모드 변경을 하지 않는다.
검은 전체화면 테스트 창이 잠깐 표시되며, Windows가 자동 전경 전환을 허용하지 않으면 60초 안에
`Codex Pulse fullscreen smoke test` 창을 활성화해야 한다. 검사 종료 또는 오류 시 테스트 창은 자동으로 닫힌다.

## 관련 자료

- 코드: `src-tauri/src/native_widget.rs`
- 사용자 설명: `README.md`
- [Microsoft SetParent 문서](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-setparent)
