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
func runPrint() throws -> PrintResponse {
    var input=Data()
    while true {
        let chunk=FileHandle.standardInput.readData(ofLength:8192)
        if chunk.isEmpty { break }
        guard chunk.count <= 65536-input.count else { throw PrintError.message("Print request is too large") }
        input.append(chunk)
    }
    let request=try JSONDecoder().decode(PrintRequest.self,from:input)
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
    let app=NSApplication.shared
    app.setActivationPolicy(.accessory)
    app.finishLaunching()
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
    guard let operation=document.printOperation(for:info,scalingMode:.pageScaleNone,autoRotate:false) else {
        throw PrintError.message("Could not create the print operation")
    }
    operation.jobTitle=request.title.isEmpty ? "Moruno drawing" : request.title
    operation.showsPrintPanel=true
    operation.showsProgressPanel=true
    operation.printPanel.options=[.showsCopies,.showsPageRange,.showsPaperSize,.showsOrientation,.showsScaling,.showsPreview]
    app.activate(ignoringOtherApps:true)
    // false means cancellation or failure; never report it as successful output.
    return PrintResponse(completed:operation.run())
}
do {
    let result=try runPrint()
    FileHandle.standardOutput.write(try JSONEncoder().encode(result))
} catch {
    let message=error is PrintError ? String(describing:error) : "Could not open the print snapshot"
    FileHandle.standardError.write(Data(message.utf8))
    exit(1)
}
