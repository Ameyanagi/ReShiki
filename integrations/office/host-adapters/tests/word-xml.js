const escape = (value) =>
  String(value).replaceAll("&", "&amp;").replaceAll('"', "&quot;").replaceAll("<", "&lt;");

export function wordPictureXml(control, pictureMarkup = '<a:xfrm rot="0"/><a:srcRect/>') {
  return `<w:sdt
    xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"
    xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"
    xmlns:pic="http://schemas.openxmlformats.org/drawingml/2006/picture"
    xmlns:wp="http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing">
    <w:sdtPr><w:id w:val="${escape(control.id)}"/><w:tag w:val="${escape(control.tag)}"/></w:sdtPr>
    <w:sdtContent><w:r><w:drawing><wp:inline><wp:docPr id="1"/>
      <a:graphic><a:graphicData uri="http://schemas.openxmlformats.org/drawingml/2006/picture">
        <pic:pic><pic:nvPicPr><pic:cNvPr id="0"/></pic:nvPicPr><pic:spPr>${pictureMarkup}</pic:spPr></pic:pic>
      </a:graphicData></a:graphic>
    </wp:inline></w:drawing></w:r></w:sdtContent>
  </w:sdt>`;
}
