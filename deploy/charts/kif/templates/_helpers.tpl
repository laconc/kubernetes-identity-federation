{{/*
Expand the name of the chart.
*/}}
{{- define "kif.name" -}}
{{- .Chart.Name | trunc 63 | trimSuffix "-" }}
{{- end }}

{{/*
Fully qualified release name. Used as the base for all resource names.
Override with fullnameOverride if needed (e.g. to keep legacy names on upgrade).
*/}}
{{- define "kif.fullname" -}}
{{- if .Values.fullnameOverride }}
{{- .Values.fullnameOverride | trunc 63 | trimSuffix "-" }}
{{- else }}
{{- .Release.Name | trunc 63 | trimSuffix "-" }}
{{- end }}
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
Compose the agent image reference: <repo>/kif-agent:<tag>
*/}}
{{- define "kif.agentImage" -}}
{{- $repo := .Values.agent.image.repository | default .Values.image.repository -}}
{{- $tag := .Values.agent.image.tag | default .Values.image.tag -}}
{{- printf "%s/kif-agent:%s" $repo $tag -}}
{{- end }}
