// Legacy bookmark compatibility only. No separate Discovery workspace/page.
import {useEffect} from "react";
import {useRouter} from "next/router";
import {legacyDestination} from "@/lib/experiment";
export default function LegacyStudies(){const router=useRouter();useEffect(()=>{if(router.isReady)router.replace(legacyDestination(router.query));},[router.isReady]);return <p>Opening optional batch and verification tools in the Laboratory…</p>;}
