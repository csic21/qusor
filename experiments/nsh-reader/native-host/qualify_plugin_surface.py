"""Exercise the real GTK window with XTest; requires a keyboard-capable X11 display."""
from pathlib import Path
import argparse,json,re,subprocess,time
from run_plugin_surface import launch,BASE
from pointer_probe import trace
from keyboard_probe import type_text
from capture_window import capture
from close_window import close

def wait(predicate,timeout=30):
    start=time.monotonic()
    while time.monotonic()-start<timeout:
        value=predicate()
        if value:return value
        time.sleep(.1)
    raise TimeoutError('native probe did not reach expected state')
def window_for(pid):
    for xid in re.findall(r'(0x[0-9a-f]+) "nsh"',subprocess.check_output(['xwininfo','-root','-tree'],text=True)):
        props=subprocess.check_output(['xprop','-id',xid,'_NET_WM_PID','WM_CLASS'],text=True)
        if f'= {pid}\n' in props and 'org.qusor.nativeprobe' in props:return xid

def main(data,evidence):
    if data.exists():raise ValueError('qualification requires a fresh data directory')
    evidence.mkdir(parents=True,exist_ok=True);log=evidence/'plugin-surface-window.log';steps=[]
    with log.open('w') as output:
        process=launch(data,output);window=None
        try:
            window=wait(lambda:window_for(process.pid));wait(lambda:'"ready":true' in log.read_text())
            capture(window,process.pid,evidence/'plugin-surface-before.png')
            selections=[(86,138,134,138,'native note one'),(152,138,186,138,'native note two'),(86,138,132,167,'native compound note')]
            for index,(x,y,end_x,end_y,note) in enumerate(selections,1):
                events=[{'kind':'move','x':x,'y':y},{'kind':'down'},{'kind':'move','x':end_x,'y':end_y},{'kind':'up'}]
                steps.append(trace(window,process.pid,events))
                wait(lambda:log.read_text().count('READER_GUEST_RESULT {"candidate_id"')>=index)
                steps.append(trace(window,process.pid,[{'kind':'move','x':790,'y':210 if index<3 else 260},{'kind':'down'},{'kind':'up'}]))
                steps.append(type_text(window,process.pid,note));time.sleep(.2)
                capture(window,process.pid,evidence/f'plugin-surface-candidate-{index}.png')
                steps.append(trace(window,process.pid,[{'kind':'move','x':790,'y':296 if index<3 else 344},{'kind':'down'},{'kind':'up'}]))
                wait(lambda:log.read_text().count('"saved":true')>=index)
                capture(window,process.pid,evidence/f'plugin-surface-saved-{index}.png')
            close(window,process.pid);exit_code=process.wait(timeout=30)
            if exit_code!=0:raise RuntimeError(f'native process exit {exit_code}')
        finally:
            if process.poll() is None:
                try:
                    if window is not None:close(window,process.pid)
                    else:process.terminate()
                    process.wait(timeout=30)
                except Exception as cleanup_error:
                    output.write(f"QUALIFICATION_CLEANUP_ERROR {cleanup_error}\n");output.flush()
                    process.terminate()
                    try:process.wait(timeout=30)
                    except subprocess.TimeoutExpired:process.kill();process.wait()
    records=[];binary=BASE.parent/'capture-core/target/debug/qusor-capture-probe'
    for index in [1,2,3]:
        command=[str(binary),'pdf-compound-query',str(data/'knowledge'),str(index)]
        records.append(json.loads(subprocess.check_output(command,text=True)))
    captures=[record['capture'] for record in records]
    assert all(record['ok'] for record in records)
    assert [record['user_note'] for record in captures]==[item[4] for item in selections]
    assert len({record['receipt']['capture_id'] for record in captures})==3
    assert len({record['receipt']['operation_id'] for record in captures})==3
    assert captures[0]['draft']['exact']=='QUSOR' and captures[1]['draft']['exact']=='native'
    assert captures[2]['compound']['protocol_version']==2 and len(captures[2]['compound']['segments'])>=2
    assert captures[2]['compound']['exact']=='QUSOR native PDF qualification\nRepeat'
    result={'display_kind' :'real GTK native window on keyboard-capable X11/Xvfb; no physical display or IME claim','data':str(data),'exit_code':exit_code,'steps':steps,'core_queries_after_process_exit':records}
    (evidence/'plugin-surface-interaction.json').write_text(json.dumps(result,ensure_ascii=False,indent=2)+'\n')
    print(json.dumps(result,ensure_ascii=False,indent=2))
if __name__=='__main__':
    parser=argparse.ArgumentParser(description=__doc__);parser.add_argument('--data',type=Path,required=True);parser.add_argument('--evidence',type=Path,required=True);args=parser.parse_args();main(args.data,args.evidence)
