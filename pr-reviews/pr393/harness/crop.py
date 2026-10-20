import sys
from PIL import Image
src, names, out = sys.argv[1], sys.argv[2].split(","), sys.argv[3]
ims=[]
for n in names:
    im=Image.open(f"{src}/{n}.png").convert("RGB")
    W,H=im.size
    ims.append(im.crop((int(W*0.25),int(H*0.35),int(W*0.55),int(H*0.95))))
w=sum(i.width for i in ims); h=max(i.height for i in ims)
o=Image.new("RGB",(w,h),"white"); x=0
for i in ims: o.paste(i,(x,0)); x+=i.width
o.save(out)
