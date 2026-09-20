import AppKit
import PDFKit
import Foundation

// One explicit print request per process. The PDF is a temporary snapshot owned
// by the editor; no document or printer settings are written back to the canvas.
struct PrintRequest: Decodable {
    let path: String
    let title: String
}
struct PrintResponse: Encodable {
    let completed: Bool
}
enum PrintError: Error, CustomStringConvertible {
    case message(String)
    var description: String {
        switch self { case .message(let text): return text }
    }
}
final class PublicationPrintView: NSView {
    let document: PDFDocument
    let pageSize: NSSize
    private var printScale: CGFloat {
        let scale=NSPrintOperation.current?.printInfo.scalingFactor ?? 1
        return scale.isFinite && scale>0 ? scale : 1
    }
    init(document: PDFDocument, pageSize: NSSize) {
        self.document=document
        self.pageSize=pageSize
        super.init(frame:NSRect(x:0,y:0,width:pageSize.width,height:pageSize.height*CGFloat(document.pageCount)))
    }
    required init?(coder:NSCoder) { return nil }
    override func knowsPageRange(_ range:NSRangePointer) -> Bool {
        range.pointee=NSRange(location:1,length:document.pageCount)
        return true
    }
    override func rectForPage(_ page:Int) -> NSRect {
        guard page>=1,page<=document.pageCount else { return .zero }
        return NSRect(x:0,y:CGFloat(page-1)*pageSize.height,width:pageSize.width,height:pageSize.height)
    }
    override func locationOfPrintRect(_ rect:NSRect) -> NSPoint {
        guard let info=NSPrintOperation.current?.printInfo else { return .zero }
        // Anchor the artwork to the physical top-left at the user's chosen scale.
        // Hardware margins can clip ink, but must never shift the drawing.
        return NSPoint(x:0,y:info.paperSize.height-rect.height*printScale)
    }
    override func draw(_ dirtyRect:NSRect) {
        guard let context=NSGraphicsContext.current?.cgContext else { return }
        for index in 0..<document.pageCount {
            let rect=rectForPage(index+1)
            guard rect.intersects(dirtyRect),let page=document.page(at:index) else { continue }
            context.saveGState()
            context.translateBy(x:rect.minX,y:rect.minY)
            // A view with explicit pagination applies the requested scale itself.
            context.scaleBy(x:printScale,y:printScale)
            page.draw(with:.mediaBox,to:context)
            context.restoreGState()
        }
    }
}
func loadSnapshot(_ request:PrintRequest) throws -> PDFDocument {
    guard !request.path.isEmpty,request.path.utf8.count<=8192,request.title.utf8.count<=1024 else {
        throw PrintError.message("Invalid print request")
    }
    let url=URL(fileURLWithPath:request.path)
    let metadata=try url.resourceValues(forKeys:[.isRegularFileKey,.fileSizeKey])
    guard metadata.isRegularFile==true,let size=metadata.fileSize,size>0,size<=128*1024*1024,
          let document=PDFDocument(url:url),!document.isLocked,document.allowsPrinting,
          document.pageCount>0,document.pageCount<=100,let first=document.page(at:0) else {
        throw PrintError.message("Could not read the print snapshot")
    }
    let bounds=first.bounds(for:.mediaBox)
    guard bounds.width.isFinite,bounds.height.isFinite,
          bounds.width>=36,bounds.height>=36,bounds.width<=2880,bounds.height<=2880 else {
        throw PrintError.message("Unsupported print page dimensions")
    }
    for index in 0..<document.pageCount {
        guard let page=document.page(at:index) else { throw PrintError.message("Missing print page") }
        let size=page.bounds(for:.mediaBox).size
        guard abs(size.width-bounds.width)<0.01,abs(size.height-bounds.height)<0.01 else {
            throw PrintError.message("Print pages must have a uniform paper size")
        }
    }
    return document
}
func printOperation(document:PDFDocument,title:String) throws -> NSPrintOperation {
    guard let first=document.page(at:0) else { throw PrintError.message("Missing print page") }
    let bounds=first.bounds(for:.mediaBox)
    let info=NSPrintInfo(dictionary:[:])
    info.orientation = .portrait
    info.paperSize=NSSize(width:min(bounds.width,bounds.height),height:max(bounds.width,bounds.height))
    info.orientation=bounds.width>bounds.height ? .landscape : .portrait
    info.scalingFactor=1
    info.leftMargin=0; info.rightMargin=0; info.topMargin=0; info.bottomMargin=0
    info.isHorizontallyCentered=false; info.isVerticallyCentered=false
    info.horizontalPagination = .clip; info.verticalPagination = .clip
    info.dictionary()[NSPrintInfo.AttributeKey.allPages]=true
    info.dictionary()[NSPrintInfo.AttributeKey.firstPage]=1
    info.dictionary()[NSPrintInfo.AttributeKey.lastPage]=document.pageCount
    // Use explicit paper coordinates. PDFKit's print view offsets artwork by
    // the printer's imageable margins even when NSPrintInfo margins are zero.
    let view=PublicationPrintView(document:document,pageSize:bounds.size)
    let operation=NSPrintOperation(view:view,printInfo:info)
    operation.jobTitle=title.isEmpty ? "Moruno drawing" : title
    operation.showsPrintPanel=true
    operation.showsProgressPanel=true
    operation.printPanel.options=[.showsCopies,.showsPageRange,.showsPaperSize,.showsOrientation,.showsScaling,.showsPreview]
    return operation
}
