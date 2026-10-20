#!/usr/bin/env python3
"""Measure supplied 8-bit, noninterlaced RGB/RGBA PNG screenshots (stdlib only).
All coordinates are original screenshot pixels, 0-based and inclusive.
"""
import struct,zlib,sys,json,collections,pathlib,hashlib

TARGET=(62,133,117)
CANVAS=(208,364,1959,1619)

def read_png(path):
 data=pathlib.Path(path).read_bytes()
 assert data[:8]==b'\x89PNG\r\n\x1a\n'
 at=8; compressed=[]
 while at<len(data):
  n=struct.unpack('>I',data[at:at+4])[0];typ=data[at+4:at+8];chunk=data[at+8:at+8+n];at+=n+12
  if typ==b'IHDR':w,h,depth,color,comp,filt,inter=struct.unpack('>IIBBBBB',chunk)
  if typ==b'IDAT':compressed.append(chunk)
 assert depth==8 and color in (2,6) and inter==0
 bpp={2:3,6:4}[color];stride=w*bpp;raw=zlib.decompress(b''.join(compressed));rows=[];pos=0;prev=bytearray(stride)
 for y in range(h):
  ft=raw[pos];pos+=1;row=bytearray(raw[pos:pos+stride]);pos+=stride
  if ft==1:
   for i in range(bpp,stride):row[i]=(row[i]+row[i-bpp])&255
  elif ft==2:
   for i in range(stride):row[i]=(row[i]+prev[i])&255
  elif ft==3:
   for i in range(stride):row[i]=(row[i]+((row[i-bpp] if i>=bpp else 0)+prev[i])//2)&255
  elif ft==4:
   for i in range(stride):
    a=row[i-bpp] if i>=bpp else 0;b=prev[i];c=prev[i-bpp] if i>=bpp else 0;p=a+b-c;pa=abs(p-a);pb=abs(p-b);pc=abs(p-c)
    row[i]=(row[i]+(a if pa<=pb and pa<=pc else b if pb<=pc else c))&255
  elif ft!=0:raise ValueError(ft)
  rows.append(bytes(row));prev=row
 def pixel(x,y):return tuple(rows[y][x*bpp:x*bpp+3])
 return w,h,rows,bpp,pixel

def runs(values):
 vals=sorted(set(values));out=[]
 for val in vals:
  if out and out[-1][1]==val-1:out[-1][1]=val
  else:out.append([val,val])
 return out

def analyze(path):
 w,h,rows,bpp,pixel=read_png(path);x0,y0,x1,y1=CANVAS
 byx=collections.defaultdict(list);byy=collections.defaultdict(list)
 for y in range(y0,y1+1):
  row=rows[y]
  for x in range(x0,x1+1):
   k=x*bpp
   if row[k:k+3]==bytes(TARGET):byx[x].append(y);byy[y].append(x)
 horizontal=[[y,a,b,b-a+1] for y,xx in sorted(byy.items()) for a,b in runs(xx) if b-a+1>=100]
 vertical=[[x,a,b,b-a+1] for x,yy in sorted(byx.items()) for a,b in runs(yy) if b-a+1>=100]
 out={'file':str(path),'size':[w,h],'target':TARGET,'canvas_inclusive':CANVAS,'exact_color_canvas_count':sum(map(len,byx.values())),'horizontal_long_runs_y_x0_x1_len':horizontal,'vertical_long_runs_x_y0_y1_len':vertical}
 # This is a deterministic handle-free *straight-run* mask, not semantic
 # segmentation of overlapping corner-handle/frame pixels.
 straight=set()
 for y,a,b,n in horizontal:straight.update((x,y) for x in range(a,b+1))
 for x,a,b,n in vertical:straight.update((x,y) for y in range(a,b+1))
 out['exact_color_long_straight_run_count']=len(straight)
 mask=bytes(1 if pixel(x,y)==TARGET else 0 for y in range(y0,y1+1) for x in range(x0,x1+1))
 out['exact_color_canvas_mask_sha256']=hashlib.sha256(mask).hexdigest()
 if horizontal and vertical:
  vx=sorted({v[0] for v in vertical});hy=sorted({v[0] for v in horizontal})
  outer=(min(vx),min(hy),max(vx),max(hy))
  out['straight_edge_exact_color_envelope_inclusive']=outer
  out['straight_edge_exact_color_envelope_size']=[outer[2]-outer[0]+1,outer[3]-outer[1]+1]
  # Midpoints of exact-color adjacent column/row groups. These are
  # raster-core midpoints and are not subpixel geometric line estimates.
  vgroups=runs(vx);hgroups=runs(hy)
  if len(vgroups)==2 and len(hgroups)==2:
   midsx=[(a+b)/2 for a,b in vgroups];midsy=[(a+b)/2 for a,b in hgroups]
   out['exact_color_core_centerline_distances']=[midsx[1]-midsx[0],midsy[1]-midsy[0]]
 border=[(x,y) for y in range(1400,1620) for x in range(650,1350) if pixel(x,y)==(214,219,223)]
 if border:
  xx,yy=zip(*border);bb=(min(xx),min(yy),max(xx),max(yy))
  out['floating_bar_exact_border_rgb']=[214,219,223]
  out['floating_bar_exact_border_envelope_inclusive']=bb
  out['floating_bar_exact_border_envelope_size']=[bb[2]-bb[0]+1,bb[3]-bb[1]+1]
  out['floating_bar_exact_border_canvas_bottom_clearance']=y1-bb[3]
  if horizontal:out['floating_bar_exact_border_gap_after_frame_core']=bb[1]-max(h[0] for h in horizontal)-1
  # Restrict scan to two pixels above the known border to exclude nearby
  # molecule and frame/handle pixels. Gray shadow at the canvas is included.
  painted=[(x,y) for y in range(bb[1]-2,y1+1) for x in range(650,1350) if pixel(x,y)!=(255,255,255)]
  xx,yy=zip(*painted);shadowbb=(min(xx),min(yy),max(xx),max(yy))
  out['floating_bar_including_shadow_nonwhite_envelope_inclusive']=shadowbb
  out['floating_bar_shadow_clipped_at_canvas_bottom']=shadowbb[3]==y1
 return out,pixel
if __name__=='__main__':
 result=[analyze(path)[0] for path in sys.argv[1:]]
 print(json.dumps({'conventions':{'coordinates':'0-based original image pixels; all rectangle endpoints inclusive','canvas':'white canvas inside 2-px RGB(225,228,232) UI borders','frame_extents':'exact RGB(62,133,117) straight-run core; excludes antialias and handles','bar_extents':'exact RGB(214,219,223) border core; excludes antialias and shadow','centerline':'midpoints of exact-color row/column groups, not geometric subpixel line estimates','long_straight_run_count':'union of all horizontal/vertical exact-color contiguous runs of length at least100; no double count'},'measurements':result},indent=2))
