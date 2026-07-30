import { Button } from "@/components/ui/button"

function App() {
  return (
    <div className="min-h-screen flex items-center justify-center bg-background">
      <div className="text-center space-y-4">
        <h1 className="text-4xl font-bold text-foreground">
          PhotoExifAnalyzer
        </h1>
        <p className="text-muted-foreground">
          分析你的照片 EXIF 信息
        </p>
        <Button>开始使用</Button>
      </div>
    </div>
  )
}

export default App
