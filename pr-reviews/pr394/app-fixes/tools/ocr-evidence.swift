import Foundation
import Vision
import ImageIO
let files=CommandLine.arguments.dropFirst()
var results:[[String:Any]]=[]
for file in files {
 let url=URL(fileURLWithPath:file)
 let request=VNRecognizeTextRequest()
 request.recognitionLevel = .accurate
 request.usesLanguageCorrection=false
 request.regionOfInterest=CGRect(x:0,y:0,width:1,height:0.05)
 let handler=VNImageRequestHandler(url:url,options:[:])
 try handler.perform([request])
 let text=(request.results ?? []).compactMap{$0.topCandidates(1).first?.string}
 results.append(["file":url.lastPathComponent,"bottom_text":text])
}
let data=try JSONSerialization.data(withJSONObject:results,options:[.prettyPrinted,.sortedKeys])
print(String(data:data,encoding:.utf8)!)
