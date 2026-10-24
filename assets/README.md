# 可选字体

egui 默认字体不含中文。应用会自动尝试 Windows 的 `msyh.ttc` / `simhei.ttf`
和 Linux 的 Noto CJK / 文泉驿字体。

如果目标机器没有中文字体，把 `NotoSansSC-Regular.ttf`（OFL-1.1）放到本目录，
打包时一并复制到可执行文件旁的 `assets/` 即可。
