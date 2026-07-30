# Bugs and Improvements

## Notes

- [ ] chưa làm, [/] đang làm, [x] hoàn thành có bằng chứng, [!] bị chặn. Không đánh dấu xong chỉ vì đã tạo khung.
- Làm tiếp tục nếu như có bug/improvement đang xử lý dở trước khi start làm bug/improvement mới.
- Không tạo mục mới với checkbox nếu mục đã có sẵn, thay vào đó sẽ đánh dấu đã xong ([x]) vào mục có sẵn đó.
- Luôn thực hiện unit test, verify kỹ càng sau khi fix bug/improvement và trước khi đánh dấu ([x]) vào mục. Là app window thì build exe và test trực tiếp trên window exe đó.
- Nếu như có version trong ngày (ví dụ ## v1.26.7.30) ở CHANGELOG.md update cả nội dung lẫn tiêu đều của version đó tương ứng: chỉ tiếng Anh, ngắn gọn, dễ hiểu cho hầu hết mọi người (cả non-tech), nếu chưa có version tương ứng ngày hiện tại thì tạo mới (gồm cả tiêu đề và nội dung). Nếu phiên bản của ngày hôm nay vẫn đang xử lý hoặc chưa xong các mục thì KHÔNG đánh dấu [x]

## Bugs

- [x] Các control của tab Translate (Providers, Excel, Dictionary, Memory) không có icon và không co lại thành icon button khi co width cửa sổ.
- [x] Do có thể mở nhiều cửa sổ nên cần xử lý việc lưu/load notes mới nhất, tránh việc combine nội dung
    - Hiện tại nếu mở 2 tab cùng 1 file, chỉnh sửa ở 2 tab rồi save lần lượt thì tab nào save sau sẽ đè nội dung tab trước, dẫn đến mất dữ liệu.
    - Cần phải có logic merge nội dung khi load file.
    - Xử lý việc merge content từ multiple windows khi load file
- [x] Khi khởi động ở Compact mode với On-top là On thì ko On-top thật, chỉ khi nhấn off -> on trở lại thì On-top mới có tác dụng.
- [x] Mapping notepad++ shortcuts theo danh sách sau:
    Command	Key
    workbench.action.toggleFullScreen	f11
    editor.foldAll	alt+0
    editor.foldLevel1	alt+1
    editor.foldLevel2	alt+2
    editor.foldLevel3	alt+3
    editor.foldLevel4	alt+4
    editor.foldLevel5	alt+5
    editor.foldLevel6	alt+6
    editor.foldLevel7	alt+7
    editor.foldLevel8	alt+8
    editor.unfoldAll	shift+alt+0
    editor.action.startFindReplaceAction	ctrl+h
    editor.action.nextMatchFindAction	f4
    editor.action.previousMatchFindAction	shift+f4
    editor.action.jumpToBracket	ctrl+b
    editor.action.clipboardCutAction	shift+delete
    undo	alt+backspace
    redo	ctrl+y
    editor.action.duplicateSelection	ctrl+d
    editor.action.joinLines	ctrl+j
    editor.action.addCommentLine	ctrl+q
    editor.action.removeCommentLine	ctrl+shift+q
    workbench.action.files.saveAll	ctrl+shift+s
    editor.action.addCommentLine	ctrl+k
    editor.action.blockComment	ctrl+shift+k
    deleteAllLeft	ctrl+shift+backspace
    workbench.action.files.saveAs	ctrl+alt+s
    workbench.action.quit	alt+f4
    workbench.action.closeActiveEditor	ctrl+w
    deleteAllRight	shift+cmd+delete
    editor.action.transformToLowercase	ctrl+u
    editor.action.transformToUppercase	ctrl+shift+u
    editor.action.jumpToBracket	ctrl+b
    cursorColumnSelectDown	shift+alt+down
    cursorColumnSelectLeft	shift+alt+left
    cursorColumnSelectPageDown	shift+alt+pagedown
    cursorColumnSelectPageUp	shift+alt+pageup
    cursorColumnSelectRight	shift+alt+right
    cursorColumnSelectUp	shift+alt+up
    workbench.action.nextEditor	ctrl+pageup
    workbench.action.previousEditor	ctrl+pagedown
    editor.action.clipboardCopyAction	ctrl+insert
    editor.action.clipboardPasteAction	shift+insert
    editor.action.moveLinesDownAction	ctrl+shift+down
    editor.action.moveLinesUpAction	ctrl+shift+up
    editor.action.deleteLines	ctrl+l
    columnSelect	alt+c