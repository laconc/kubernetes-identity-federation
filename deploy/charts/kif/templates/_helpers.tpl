{{/*
Expand the name of the chart.
*/}}
{{- define "kif.name" -}}
{{- .Chart.Name | trunc 63 | trimSuffix "-" }}
{{- end }}

{{/*
Common labels
*/}}
{{- define "kif.labels" -}}
helm.sh/chart: {{ printf "%s-%s" .Chart.Name .Chart.Version | replace "+" "_" | trunc 63 | trimSuffix "-" }}
{{ include "kif.selectorLabels" . }}
app.kubernetes.io/managed-by: {{ .Release.Service }}
{{- end }}

{{/*
Selector labels
*/}}
{{- define "kif.selectorLabels" -}}
app.kubernetes.io/name: {{ include "kif.name" . }}
app.kubernetes.io/instance: {{ .Release.Name }}
{{- end }}

{{/*
Resolve image for a given component dict.
Usage: {{ include "kif.image" (dict "component" .Values.federation "global" .Values) }}
*/}}
{{- define "kif.image" -}}
{{- $repo := .component.image.repository | default .global.image.repository -}}
{{- $tag := .component.image.tag | default .global.image.tag -}}
{{- $pullPolicy := .component.image.pullPolicy | default .global.image.pullPolicy -}}
{{- printf "%s/%s:%s" $repo .name $tag -}}
{{- end }}

{{/*
Resolve image pull policy for a given component.
*/}}
{{- define "kif.pullPolicy" -}}
{{- .component.image.pullPolicy | default .global.image.pullPolicy -}}
{{- end }}

{{/*
Compose the agent image reference: <repo>/kif-agent:<tag>
*/}}
{{- define "kif.agentImage" -}}
{{- $repo := .Values.agent.image.repository | default .Values.image.repository -}}
{{- $tag := .Values.agent.image.tag | default .Values.image.tag -}}
{{- printf "%s/kif-agent:%s" $repo $tag -}}
{{- end }}
