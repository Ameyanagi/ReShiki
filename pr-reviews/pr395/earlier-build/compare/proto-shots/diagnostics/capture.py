import subprocess,time,json,sys
from pathlib import Path
OUT=Path(__file__).parent
UI=str(OUT/'prototype-input')
WID='7128'
MOLECULES={'caffeine':(1027,249),'ibuprofen':(1082,249)}
LOOKS={'flat':(1027,625),'glossy':(1130,625)}
STYLES={'ballstick':(1027,540),'stick':(1130,540),'wire':(1027,571),'space':(1130,571)}
def input(*args): subprocess.run([UI,*map(str,args)],check=True)
def click(p): input('click',*p)
def capture(name,meta):
    input('park'); time.sleep(.6)
    bounds=subprocess.check_output([UI,'info'],text=True).strip()
    subprocess.run(['screencapture','-x','-o','-l',WID,str(OUT/(name+'.png'))],check=True)
    with (OUT/'capture-log.jsonl').open('a') as f: f.write(json.dumps({'file':name+'.png','time':time.strftime('%Y-%m-%d %H:%M:%S %z'),'bounds':bounds,**meta})+'\n')
    print(name,flush=True)
if __name__=='__main__':
    mol,look=sys.argv[1:3]
    input('focus')
    input('scroll',5000); time.sleep(.25)
    click(MOLECULES[mol]); time.sleep(1.1)
    click(LOOKS[look])
    for style,pos in STYLES.items():
        click(pos)
        capture(f'{mol}-{style}-{look}',{'mol':mol,'look':look,'style':style,'show_as':'3D','scroll':'top'})
