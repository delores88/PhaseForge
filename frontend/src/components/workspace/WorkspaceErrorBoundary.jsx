import {Component} from 'react';
import {recordClientError} from '@/lib/clientDiagnostics.mjs';
import styles from './WorkspaceErrorBoundary.module.css';

/** Keep navigation, the composer and saved jobs independent of a failed view. */
export default class WorkspaceErrorBoundary extends Component{
  constructor(props){super(props);this.state={error:null,resetKey:props.resetKey};}
  static getDerivedStateFromProps(props,state){return props.resetKey!==state.resetKey?{error:null,resetKey:props.resetKey}:null;}
  static getDerivedStateFromError(error){return{error};}
  componentDidCatch(error,info){recordClientError(error,{scope:this.props.scope||'workspace',componentStack:info.componentStack});}
  render(){
    if(!this.state.error)return this.props.children;
    if(this.props.fallback!==undefined)return this.props.fallback;
    return <section className={styles.fallback} role="alert"><strong>{this.props.title||'This view could not be displayed.'}</strong><p>Saved jobs and files remain available. You can switch views or retry this view.</p><button type="button" onClick={()=>this.setState({error:null})}>Retry view</button><details><summary>Error details</summary><pre>{String(this.state.error?.message||'Unknown display error')}</pre></details></section>;
  }
}
