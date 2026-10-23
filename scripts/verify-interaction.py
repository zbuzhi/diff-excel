#!/usr/bin/env python3
"""Independent ZIP/XML check of interaction_benchmark's generated report."""
import re
import sys
import zipfile
from pathlib import Path
from xml.etree import ElementTree as ET

NS = {'s': 'http://schemas.openxmlformats.org/spreadsheetml/2006/main'}

def read(path):
    with zipfile.ZipFile(path) as archive:
        strings = []
        if 'xl/sharedStrings.xml' in archive.namelist():
            strings = [''.join(si.itertext()) for si in ET.fromstring(archive.read('xl/sharedStrings.xml'))]
        sheet = ET.fromstring(archive.read('xl/worksheets/sheet1.xml'))
        cells = {}
        styles = {}
        for cell in sheet.findall('.//s:sheetData/s:row/s:c', NS):
            value = cell.findtext('s:v', default='', namespaces=NS)
            kind = cell.get('t', 'n')
            if kind == 's':
                value = strings[int(value)]
            elif kind == 'inlineStr':
                value = ''.join(cell.find('s:is', NS).itertext())
            cells[cell.get('r')] = (kind, value, cell.findtext('s:f', default='', namespaces=NS))
            styles[cell.get('r')] = int(cell.get('s', '0'))
        return cells, styles

root = Path(sys.argv[1] if len(sys.argv) > 1 else '/tmp/diff-excel-interaction-qa')
report = sorted(root.glob('reports/comparison-*/diff.xlsx'), key=lambda p: p.stat().st_mtime)[-1]
old, _ = read(root / '旧版本-10000x20.xlsx')
new, _ = read(root / '新版本-10000x20.xlsx')
output, styles = read(report)
expected = {f'B{row}' for row in range(2, 10001)}
actual = {cell for cell in old.keys() | new.keys() if old.get(cell) != new.get(cell)}
assert actual == expected, (len(actual), len(expected))
assert output == new, '导出改变了新版本数据、类型或公式'
with zipfile.ZipFile(report) as archive:
    style = ET.fromstring(archive.read('xl/styles.xml'))
    fills = list(style.find('s:fills', NS))
    xfs = list(style.find('s:cellXfs', NS))
    for cell in expected:
        fill = fills[int(xfs[styles[cell]].get('fillId'))]
        assert fill.find('s:patternFill/s:fgColor', NS).get('rgb') == 'FF874BAC', cell
    comments = {}
    for name in archive.namelist():
        if 'comment' in name.lower() and name.endswith('.xml'):
            tree = ET.fromstring(archive.read(name))
            for comment in tree.findall('.//s:comment', NS):
                comments[comment.get('ref')] = ''.join(comment.itertext())
    assert set(comments) == expected
    for cell in expected:
        assert f'客户{cell[1:]}' in comments[cell]
log = report.with_name('changes.txt').read_text()
assert len(re.findall(r'^\[修改\]', log, flags=re.M)) == 9999
print('PASS: 200,000 个单元格值/类型/公式，9,999 个变化坐标、高亮、批注及日志全部核对通过')
