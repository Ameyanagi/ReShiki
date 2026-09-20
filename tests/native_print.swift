import AppKit
import PDFKit
import Foundation

struct TestFailure: Error { let message:String }
func check(_ condition:Bool,_ message:String) throws {
    if !condition { throw TestFailure(message:message) }
}
func makePDF(at url:URL,size:CGSize,marks:[CGRect]) throws {
    var bounds=CGRect(origin:.zero,size:size)
    guard let context=CGContext(url as CFURL,mediaBox:&bounds,nil) else { throw TestFailure(message:"PDF context unavailable") }
    for mark in marks {
        context.beginPDFPage(nil)
        context.setFillColor(CGColor(gray:0,alpha:1))
        context.fill(mark)
        context.endPDFPage()
    }
    context.closePDF()
}
func inkBounds(_ page:PDFPage) throws -> CGRect {
    let size=page.bounds(for:.mediaBox).size
    let width=Int(ceil(size.width*2)),height=Int(ceil(size.height*2))
    guard let context=CGContext(data:nil,width:width,height:height,bitsPerComponent:8,bytesPerRow:width*4,
                               space:CGColorSpaceCreateDeviceRGB(),bitmapInfo:CGImageAlphaInfo.premultipliedLast.rawValue) else {
        throw TestFailure(message:"Bitmap context unavailable")
    }
    context.setFillColor(CGColor(gray:1,alpha:1));context.fill(CGRect(x:0,y:0,width:width,height:height))
    context.scaleBy(x:2,y:2);page.draw(with:.mediaBox,to:context)
    guard let data=context.makeImage()?.dataProvider?.data else { throw TestFailure(message:"Bitmap data unavailable") }
    let bytes=data as Data
    var left=width,right=0,top=height,bottom=0
    for (offset,value) in bytes.enumerated() where offset%4==0 && value<128 {
        let x=(offset/4)%width,y=(offset/4)/width
        left=min(left,x);right=max(right,x+1);top=min(top,y);bottom=max(bottom,y+1)
    }
    try check(left<right && top<bottom,"Printed page is blank")
    return CGRect(x:left,y:top,width:right-left,height:bottom-top)
}

@main
struct NativePrintTests {
    static func main() {
        do { try run() }
        catch {
            FileHandle.standardError.write(Data("\(error)\n".utf8))
            exit(1)
        }
    }
    static func run() throws {
        let app=NSApplication.shared
        app.setActivationPolicy(.prohibited)
        app.finishLaunching()
        let directory=FileManager.default.temporaryDirectory.appendingPathComponent("moruno-print-tests-"+UUID().uuidString)
        try FileManager.default.createDirectory(at:directory,withIntermediateDirectories:true)
        defer { try? FileManager.default.removeItem(at:directory) }
        let sizes=[CGSize(width:595.2756,height:841.8898),CGSize(width:792,height:612),CGSize(width:360,height:480)]
        let marks=[CGRect(x:60,y:100,width:28.8,height:14.4),CGRect(x:90,y:140,width:43.2,height:28.8)]
        for (index,size) in sizes.enumerated() {
            let source=directory.appendingPathComponent("source.pdf")
            try makePDF(at:source,size:size,marks:marks)
            let document=try loadSnapshot(PrintRequest(path:source.path,title:"Regression"))
            for scale in [1.0,0.5] {
                for rangeOnly in [false,true] {
                    let output=directory.appendingPathComponent("printed-\(index)-\(scale)-\(rangeOnly).pdf")
                    let expected=directory.appendingPathComponent("expected.pdf")
                    let selected=rangeOnly ? Array(marks.suffix(1)) : marks
                    try makePDF(at:expected,size:size,marks:selected.map {
                        CGRect(x:$0.minX*scale,y:size.height*(1-scale)+$0.minY*scale,width:$0.width*scale,height:$0.height*scale)
                    })
                    let operation=try printOperation(document:document,title:"Print regression")
                    try check(operation.printInfo.scalingFactor==1,"Default print scale must be 100%")
                    operation.showsPrintPanel=false;operation.showsProgressPanel=false
                    // Always save to a private file. This test never submits a printer job.
                    operation.printInfo.jobDisposition = .save
                    operation.printInfo.dictionary()[NSPrintInfo.AttributeKey.jobSavingURL]=output
                    operation.printInfo.scalingFactor=scale
                    if rangeOnly {
                        operation.printInfo.dictionary()[NSPrintInfo.AttributeKey.allPages]=false
                        operation.printInfo.dictionary()[NSPrintInfo.AttributeKey.firstPage]=2
                        operation.printInfo.dictionary()[NSPrintInfo.AttributeKey.lastPage]=2
                    }
                    try check(operation.run(),"Save-to-PDF print operation failed")
                    guard let result=PDFDocument(url:output),let reference=PDFDocument(url:expected) else { throw TestFailure(message:"Missing printed PDF") }
                    try check(result.pageCount==selected.count,"Wrong printed page count")
                    for pageIndex in 0..<result.pageCount {
                        guard let page=result.page(at:pageIndex),let ref=reference.page(at:pageIndex) else { throw TestFailure(message:"Missing page") }
                        let bounds=page.bounds(for:.mediaBox)
                        try check(abs(bounds.width-size.width)<0.01 && abs(bounds.height-size.height)<0.01,"Paper dimensions changed: expected=\(size) actual=\(bounds.size) rotation=\(page.rotation) settings=\(operation.printInfo.paperSize)")
                        let actual=try inkBounds(page),desired=try inkBounds(ref)
                        try check(abs(actual.minX-desired.minX)<=1 && abs(actual.minY-desired.minY)<=1 && abs(actual.width-desired.width)<=1 && abs(actual.height-desired.height)<=1,
                                  "Artwork shifted or scaled: paper=\(size) scale=\(scale) page=\(pageIndex) actual=\(actual) expected=\(desired)")
                    }
                }
            }
        }
        print("Native printing: portrait, landscape, custom paper, 100%/50% scale, page order and page ranges passed")
    }
}
